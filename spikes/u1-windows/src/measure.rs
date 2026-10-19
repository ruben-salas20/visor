//! Procesos descendientes del proceso actual y su memoria (Working Set y Private Bytes).

use std::collections::{HashMap, HashSet};
use std::mem::size_of;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};

pub struct Proceso {
    pub pid: u32,
    pub padre: u32,
    pub nombre: String,
    pub working_set: Option<u64>,
    pub private_bytes: Option<u64>,
}

pub struct Medida {
    pub host: Proceso,
    pub hijos: Vec<Proceso>,
}

pub fn medir(raiz: u32) -> Result<Medida, String> {
    let procesos = instantanea()?;

    let mut por_padre: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, p) in procesos.iter().enumerate() {
        por_padre.entry(p.1).or_default().push(i);
    }

    // Recorrido por anchura con vistos: un PID reutilizado no puede crear un bucle.
    let mut hijos = Vec::new();
    let mut vistos = HashSet::from([raiz]);
    let mut pendientes = vec![raiz];
    while let Some(pid) = pendientes.pop() {
        let Some(indices) = por_padre.get(&pid) else {
            continue;
        };
        for &i in indices {
            let (hijo, padre, nombre) = &procesos[i];
            if vistos.insert(*hijo) {
                pendientes.push(*hijo);
                let (ws, pb) = memoria(*hijo);
                hijos.push(Proceso {
                    pid: *hijo,
                    padre: *padre,
                    nombre: nombre.clone(),
                    working_set: ws,
                    private_bytes: pb,
                });
            }
        }
    }

    let (ws, pb) = memoria(raiz);
    let host = Proceso {
        pid: raiz,
        padre: 0,
        nombre: "host (este proceso)".into(),
        working_set: ws,
        private_bytes: pb,
    };
    Ok(Medida { host, hijos })
}

/// Lista (pid, padre, nombre) de todos los procesos del sistema.
fn instantanea() -> Result<Vec<(u32, u32, String)>, String> {
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return Err("CreateToolhelp32Snapshot falló".into());
    }
    let mut out = Vec::new();
    let mut entrada: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entrada.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    unsafe {
        if Process32FirstW(snap, &mut entrada) != 0 {
            loop {
                let largo = entrada
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entrada.szExeFile.len());
                let nombre = String::from_utf16_lossy(&entrada.szExeFile[..largo]);
                out.push((entrada.th32ProcessID, entrada.th32ParentProcessID, nombre));
                if Process32NextW(snap, &mut entrada) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    Ok(out)
}

/// (Working Set, Private Bytes) en bytes; `None` si no se puede leer el proceso.
fn memoria(pid: u32) -> (Option<u64>, Option<u64>) {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if h.is_null() {
            return (None, None);
        }
        let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        c.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let ok = K32GetProcessMemoryInfo(h, &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb);
        CloseHandle(h);
        if ok == 0 {
            (None, None)
        } else {
            (Some(c.WorkingSetSize as u64), Some(c.PrivateUsage as u64))
        }
    }
}
