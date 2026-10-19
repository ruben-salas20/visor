//! The test sequence, the same on both systems. It runs on its own thread and
//! drives real input (a virtual pointer on Wayland, SendInput on Windows); it
//! only reads what each view sent through its own IPC handler.

use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Every IPC message, tagged with the view whose handler received it.
pub type Registro = Arc<Mutex<Vec<(&'static str, Value)>>>;

pub trait Entrada {
    /// Window coordinates, in CSS pixels.
    fn mover(&mut self, x: i32, y: i32);
    fn pulsar(&mut self);
    fn soltar(&mut self);
    fn rueda(&mut self, pasos: i32);
    fn teclear(&mut self, texto: &str);
    /// Colour on screen at a window point.
    fn pixel(&mut self, x: i32, y: i32) -> Option<[u8; 3]>;
    fn redimensionar(&mut self, w: i32, h: i32);
    /// Saves a screenshot, if the system can.
    fn captura(&mut self, _nombre: &str) {}
}

const VERDE: [u8; 3] = [0x2e, 0x7d, 0x32];
const ROSA: [u8; 3] = [0xf0, 0xd0, 0xd0];
const AZUL: [u8; 3] = [0x2b, 0x57, 0x97];
const LADO: [u8; 3] = [0xe8, 0xe4, 0xda];

fn cerca(a: Option<[u8; 3]>, b: [u8; 3]) -> bool {
    a.is_some_and(|a| a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= 24))
}

fn hex(p: Option<[u8; 3]>) -> Value {
    p.map(|p| json!(format!("#{:02x}{:02x}{:02x}", p[0], p[1], p[2]))).unwrap_or(Value::Null)
}

fn pausa(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

struct Prueba<'a> {
    reg: Registro,
    e: &'a mut dyn Entrada,
    out: Map<String, Value>,
    fallos: Vec<String>,
}

impl Prueba<'_> {
    fn marca(&self) -> usize {
        self.reg.lock().unwrap().len()
    }

    fn desde(&self, n: usize) -> Vec<(&'static str, Value)> {
        self.reg.lock().unwrap()[n..].to_vec()
    }

    fn esperar(&self, n: usize, ms: u64, f: impl Fn(&str, &Value) -> bool) -> bool {
        let fin = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < fin {
            if self.desde(n).iter().any(|(v, m)| f(v, m)) {
                return true;
            }
            pausa(20);
        }
        false
    }

    fn hay(&self, n: usize, vista: &str, tipo: &str) -> bool {
        self.desde(n).iter().any(|(v, m)| *v == vista && m["tipo"] == tipo)
    }

    fn clic(&mut self, x: i32, y: i32) {
        self.e.mover(x, y);
        pausa(80);
        self.e.pulsar();
        pausa(60);
        self.e.soltar();
    }

    fn anota(&mut self, clave: &str, ok: bool, detalle: Value) {
        if !ok {
            self.fallos.push(clave.to_string());
        }
        self.out.insert(clave.into(), json!({ "ok": ok, "detalle": detalle }));
    }

    fn tipos(&self, n: usize) -> Value {
        json!(self.desde(n).iter().map(|(v, m)| format!("{v}:{}", m["tipo"].as_str().unwrap_or("?"))).collect::<Vec<_>>())
    }
}

/// Runs every step and returns the JSON report.
pub fn ejecutar(reg: Registro, e: &mut dyn Entrada) -> Value {
    let mut p = Prueba { reg, e, out: Map::new(), fallos: Vec::new() };
    let listos = {
        let fin = Instant::now() + Duration::from_secs(30);
        loop {
            if p.hay(0, "marco", "estado") && p.hay(0, "pagina", "listo") {
                break true;
            }
            if Instant::now() > fin {
                break false;
            }
            pausa(50);
        }
    };
    p.anota("0-cargadas", listos, p.tipos(0));
    if !listos {
        return json!({ "ok": false, "pasos": p.out, "fallos": p.fallos });
    }
    pausa(800);

    // 1. Menu closed: the page shows through the frame's hole.
    let (a, b, c) = (p.e.pixel(340, 144), p.e.pixel(700, 500), p.e.pixel(100, 400));
    p.anota("1-hueco-transparente", cerca(a, VERDE) && cerca(b, ROSA) && cerca(c, LADO),
            json!({ "boton_pagina": hex(a), "fondo_pagina": hex(b), "lado_marco": hex(c) }));

    // 2. Open the menu with a real click on the frame.
    let n = p.marca();
    p.clic(60, 20);
    let abierto = p.esperar(n, 3000, |v, m| v == "marco" && m["tipo"] == "menu" && m["abierto"] == true);
    pausa(400);
    let (a, b, s) = (p.e.pixel(340, 144), p.e.pixel(700, 500), p.e.pixel(340, 258));
    p.e.captura("menu-abierto");
    let sombra = s.is_some_and(|s| (s[1] as i32) < VERDE[1] as i32 - 15);
    p.anota("2-menu-encima", abierto && cerca(a, AZUL) && cerca(b, ROSA),
            json!({ "abierto": abierto, "menu_sobre_pagina": hex(a), "pagina_fuera_del_menu": hex(b), "eventos": p.tipos(n) }));
    p.anota("2b-sombra-sobre-pagina", sombra, json!({ "pixel_bajo_el_menu": hex(s), "verde_sin_sombra": hex(Some(VERDE)) }));

    // 3. Click an item that lies over the page: the frame gets it, the page does not.
    let n = p.marca();
    p.clic(340, 144);
    let item = p.esperar(n, 3000, |v, m| v == "marco" && m["tipo"] == "item" && m["n"] == 2);
    pausa(300);
    let pagina = p.hay(n, "pagina", "clic-pagina");
    p.anota("3-clic-en-menu", item && !pagina, json!({ "eventos": p.tipos(n) }));

    // 4. Menu closed: the same point reaches the page.
    pausa(300);
    let n = p.marca();
    p.clic(340, 144);
    let pagina = p.esperar(n, 3000, |v, m| v == "pagina" && m["tipo"] == "clic-pagina");
    let marco = p.hay(n, "marco", "clic-marco");
    p.anota("4-clic-en-pagina", pagina && !marco, json!({ "eventos": p.tipos(n) }));

    // 5. Wheel over the page.
    let n = p.marca();
    p.e.mover(700, 500);
    pausa(100);
    p.e.rueda(5);
    let rueda = p.esperar(n, 3000, |v, m| v == "pagina" && m["tipo"] == "scroll" && m["y"].as_f64().unwrap_or(0.0) > 0.0);
    p.anota("5-rueda-en-pagina", rueda, json!({ "eventos": p.tipos(n) }));

    // 6. Keyboard in the page's field.
    let n = p.marca();
    p.clic(720, 75);
    pausa(300);
    p.e.teclear("hola");
    let texto = p.esperar(n, 3000, |v, m| v == "pagina" && m["tipo"] == "texto" && m["valor"] == "hola");
    let marco = p.hay(n, "marco", "tecla-marco");
    p.anota("6-teclado-en-pagina", texto && !marco, json!({ "eventos": p.tipos(n) }));

    // 7. Open menu, click the page outside the menu: the frame loses focus and closes it.
    let n = p.marca();
    p.clic(60, 20);
    let abierto = p.esperar(n, 3000, |v, m| v == "marco" && m["tipo"] == "menu" && m["abierto"] == true);
    pausa(300);
    let n2 = p.marca();
    p.clic(700, 500);
    let cerrado = p.esperar(n2, 3000, |v, m| v == "marco" && m["tipo"] == "cerrado-por-foco");
    pausa(300);
    let a = p.e.pixel(340, 144);
    p.anota("7-cierra-al-pulsar-pagina", abierto && cerrado && cerca(a, VERDE), json!({ "eventos": p.tipos(n), "pixel_tras_cerrar": hex(a) }));

    // 8. Drag from the frame's side panel onto the page area.
    let n = p.marca();
    p.e.mover(100, 75);
    pausa(100);
    p.e.pulsar();
    for i in 1..=20 {
        p.e.mover(100 + i * 30, 75 + i * 21);
        pausa(30);
    }
    pausa(200);
    p.e.soltar();
    pausa(800);
    let en_marco = p.hay(n, "marco", "soltar-en-marco");
    let en_pagina = p.hay(n, "pagina", "soltar-en-pagina");
    p.out.insert("8-arrastre-al-hueco".into(), json!({ "informativo": true, "soltar_en_marco": en_marco, "soltar_en_pagina": en_pagina, "eventos": p.tipos(n) }));

    // 9. The page cannot reach the frame: its forged order stays in its own handler.
    let reg = p.desde(0);
    let falsas_en_marco = reg.iter().any(|(v, m)| *v == "marco" && m.get("falsificado").is_some());
    let falsas_en_pagina = reg.iter().any(|(v, m)| *v == "pagina" && m.get("falsificado").is_some());
    let alcanza = reg.iter().find(|(v, m)| *v == "pagina" && m["tipo"] == "listo").map(|(_, m)| m["alcanza"].clone()).unwrap_or(Value::Null);
    let aislada = !falsas_en_marco && falsas_en_pagina && alcanza.as_array().is_some_and(|a| a.is_empty());
    p.anota("9-pagina-aislada", aislada, json!({ "falsa_llega_al_marco": falsas_en_marco, "falsa_en_su_manejador": falsas_en_pagina, "alcanza": alcanza }));

    // 10. Resize: the region follows the new holes.
    let n = p.marca();
    p.e.redimensionar(900, 600);
    let nuevo = p.esperar(n, 4000, |v, m| v == "marco" && m["tipo"] == "estado" && m["ventana"] == json!([900, 600]));
    pausa(500);
    let n2 = p.marca();
    p.clic(340, 144);
    let pagina = p.esperar(n2, 3000, |v, m| v == "pagina" && m["tipo"] == "clic-pagina");
    let n3 = p.marca();
    p.clic(60, 20);
    let abierto = p.esperar(n3, 3000, |v, m| v == "marco" && m["tipo"] == "menu" && m["abierto"] == true);
    pausa(400);
    let a = p.e.pixel(340, 144);
    p.anota("10-tras-redimensionar", nuevo && pagina && abierto && cerca(a, AZUL), json!({ "eventos": p.tipos(n), "menu": hex(a) }));

    let ok = p.fallos.is_empty();
    json!({ "ok": ok, "fallos": p.fallos, "pasos": p.out })
}
