# Prueba de humo del visor real en Windows (CI, .github/workflows/windows-host.yml).
#
# Abre visor con su interfaz sobre una bóveda pequeña, en una instancia aislada
# (VISOR_SOCKET propio), y comprueba por la CLI: que responde, que abre una nota
# y una página HTML, que eval y screenshot funcionan en una página de la bóveda,
# que una página hostil de la bóveda y una web no consiguen nada, y que la ventana
# se cierra limpia. Con -ExeW abre visor como el Explorador, con visorw.exe, y
# comprueba además que la app no tiene consola, que `visor mcp` responde por
# stdio y que visorw.exe sin visor.exe avisa. Deja en $Salida el informe
# (informe.json) y las capturas.
#
# Uso: smoke.ps1 -Exe <visor.exe> [-ExeW <visorw.exe>] -Salida <carpeta>

param(
    [Parameter(Mandatory)] [string] $Exe,
    [string] $ExeW,
    [Parameter(Mandatory)] [string] $Salida
)

$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force -Path $Salida | Out-Null
$Salida = (Resolve-Path $Salida).Path
$tmp = Join-Path $env:RUNNER_TEMP 'visor-humo'
Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
$boveda = Join-Path $tmp 'boveda'
$web = Join-Path $tmp 'web'
New-Item -ItemType Directory -Force -Path $boveda, $web | Out-Null

# Instancia aislada: su propio canal; sin buscar actualizaciones.
$env:VISOR_SOCKET = Join-Path $tmp 'visor.sock'
$config = Join-Path $env:APPDATA 'visor'
New-Item -ItemType Directory -Force -Path $config | Out-Null
Set-Content -Path (Join-Path $config 'config.json') -Value '{"auto_update": false}' -Encoding utf8

Set-Content -Path (Join-Path $boveda 'nota.md') -Encoding utf8 -Value "# Nota de prueba`n`nTexto de la nota."
Set-Content -Path (Join-Path $boveda 'pagina.html') -Encoding utf8 -Value '<!doctype html><html lang="es"><meta charset="utf-8"><title>Página</title><body style="background:#2a6"><h1>Página HTML</h1><script>addEventListener("click", () => { const d = document.documentElement.dataset; d.clics = (+d.clics || 0) + 1; });</script></body></html>'
# Una página de la bóveda que intenta lo que no le toca: mensajes sin clave,
# órdenes del marco por el canal de las páginas y una clave inventada.
Set-Content -Path (Join-Path $boveda 'hostil.html') -Encoding utf8 -Value @'
<!doctype html><html lang="es"><meta charset="utf-8"><title>hostil</title><body><h1>Hostil</h1><script>
const raw = (m) => { try { (window.chrome && window.chrome.webview ? window.chrome.webview : window.ipc).postMessage(JSON.stringify(m)); } catch (e) {} };
raw({ type: 'create_note', dir: '', name: 'sin-clave' });
raw({ type: 'save_note', text: 'escrito sin clave' });
raw({ type: 'create_note', dir: '', name: 'clave-inventada', key: 'inventada' });
raw({ area: 'sides', type: 'new_tab' });
raw({ area: 'sides', type: 'new_tab', key: 'inventada' });
raw({ area: 'frame', type: 'ready' });
if (window.__visorPost) { window.__visorPost({ area: 'sides', type: 'new_tab' }); window.__visorPost({ area: 'files', type: 'new_note', dir: '' }); }
document.title = 'hostil listo';
</script></body></html>
'@
# Una web (otro origen, fuera de la bóveda) que intenta escribir en la bóveda.
Set-Content -Path (Join-Path $web 'sitio.html') -Encoding utf8 -Value @'
<!doctype html><html lang="es"><meta charset="utf-8"><title>sitio</title><body><h1>Sitio</h1><script>
const raw = (m) => { try { (window.chrome && window.chrome.webview ? window.chrome.webview : window.ipc).postMessage(JSON.stringify(m)); } catch (e) {} };
const post = window.__visorPost;
if (post) { post({ type: 'create_note', dir: '', name: 'desde-web' }); post({ type: 'save_note', text: 'escrito desde la web' }); }
raw({ type: 'create_note', dir: '', name: 'desde-web-sin-clave' });
raw({ area: 'sides', type: 'new_tab' });
document.title = post ? 'sitio con __visorPost' : 'sitio sin __visorPost';
</script></body></html>
'@
$notaAntes = Get-Content -Raw (Join-Path $boveda 'nota.md')

$comprobaciones = [System.Collections.Generic.List[object]]::new()
function Comprobar([string] $nombre, [bool] $ok, $dato) {
    $comprobaciones.Add([pscustomobject]@{ nombre = $nombre; ok = $ok; dato = $dato })
    Write-Host ("[{0}] {1}: {2}" -f ($(if ($ok) { 'ok' } else { 'FALLO' }), $nombre, ($dato | ConvertTo-Json -Compress -Depth 6)))
}

function Visor([string[]] $argumentos) {
    $texto = (& $Exe @argumentos 2>&1 | Out-String)
    try { return $texto | ConvertFrom-Json } catch { return [pscustomobject]@{ ok = $false; error = "no es JSON: $texto" } }
}

# Las consolas que Windows abrió para el proceso $id: conhost.exe (u OpenConsole.exe)
# arranca como hijo del primer programa de consola que la necesita.
function Consolas([int] $id) {
    return @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $id" | Where-Object { $_.Name -in 'conhost.exe', 'OpenConsole.exe' } | ForEach-Object Name)
}

function Pestanas($estado) {
    $n = 0
    foreach ($lado in @($estado.sides.list)) { $n += @($lado.tabs).Count }
    return $n
}

function Captura([string] $ruta) {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $pantalla = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $imagen = New-Object System.Drawing.Bitmap $pantalla.Width, $pantalla.Height
    $g = [System.Drawing.Graphics]::FromImage($imagen)
    $g.CopyFromScreen($pantalla.Location, [System.Drawing.Point]::Empty, $pantalla.Size)
    $imagen.Save($ruta, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $imagen.Dispose()
}

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Raton {
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
}
'@

# Un clic real en un punto de la ventana, relativo a su esquina (escala 1 en el ejecutor).
function Clic($proceso, [int] $x, [int] $y) {
    $proceso.Refresh()
    $h = $proceso.MainWindowHandle
    $r = New-Object Raton+RECT
    $null = [Raton]::GetWindowRect($h, [ref] $r)
    $null = [Raton]::SetForegroundWindow($h)
    $null = [Raton]::SetCursorPos($r.Left + $x, $r.Top + $y)
    Start-Sleep -Milliseconds 200
    [Raton]::mouse_event(0x02, 0, 0, 0, [UIntPtr]::Zero)
    [Raton]::mouse_event(0x04, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 700
}

$servidor = $null
$proceso = $null
$lanzador = $null
try {
    if ($ExeW) {
        # Como el Explorador: visorw.exe arranca visor.exe y termina enseguida.
        $ExeW = (Resolve-Path $ExeW).Path
        $lanzador = Start-Process -FilePath $ExeW -PassThru
        $termino = $lanzador.WaitForExit(10000)
        Comprobar 'visorw.exe arranca visor y termina' ($termino -and $lanzador.ExitCode -eq 0) @{ termino = $termino; codigo = $(if ($termino) { $lanzador.ExitCode } else { $null }) }
    }
    else {
        $proceso = Start-Process -FilePath $Exe -PassThru -RedirectStandardOutput (Join-Path $Salida 'visor.stdout.txt') -RedirectStandardError (Join-Path $Salida 'visor.stderr.txt')
    }

    # 1. La instancia responde.
    $estado = $null
    $limite = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $limite -and -not ($proceso -and $proceso.HasExited)) {
        $estado = Visor @('status')
        if ($estado.ok -eq $true) { break }
        Start-Sleep -Milliseconds 500
    }
    Comprobar 'visor status responde' ($estado.ok -eq $true) $estado
    if ($estado.ok -ne $true) { throw 'visor no respondió en 60 s' }

    if ($lanzador) {
        $app = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($lanzador.Id) AND Name = 'visor.exe'")
        Comprobar 'visorw.exe arrancó un solo visor.exe' ($app.Count -eq 1) @($app | ForEach-Object ProcessId)
        if ($app.Count -ne 1) { throw 'no se encuentra el visor.exe que arrancó visorw.exe' }
        $proceso = Get-Process -Id $app[0].ProcessId
        # Abre el proceso ya, para poder leer su código de salida al cerrarse.
        $null = $proceso.Handle
        $consolas = Consolas $proceso.Id
        Comprobar 'la app abierta con visorw.exe no tiene consola' ($consolas.Count -eq 0) $consolas
        # Control: el mismo visor.exe abierto desde el Explorador sí la tiene.
        $control = Start-Process -FilePath $Exe -ArgumentList 'mcp' -PassThru
        Start-Sleep -Seconds 2
        $consolas = Consolas $control.Id
        Comprobar 'control: visor.exe abierto directamente tiene consola' ($consolas.Count -ge 1) $consolas
        taskkill /T /F /PID $control.Id | Out-Null
    }

    # MCP por stdio: initialize y salida limpia al cerrar la entrada.
    $info = [System.Diagnostics.ProcessStartInfo]::new($Exe, 'mcp')
    $info.UseShellExecute = $false
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $mcp = [System.Diagnostics.Process]::Start($info)
    $mcp.StandardInput.WriteLine('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"humo","version":"0"}}}')
    $mcp.StandardInput.Flush()
    $linea = $mcp.StandardOutput.ReadLineAsync()
    $llego = $linea.Wait(10000)
    $respuesta = $null
    if ($llego) { try { $respuesta = $linea.Result | ConvertFrom-Json } catch {} }
    Comprobar 'visor mcp responde a initialize por stdio' ($llego -and $respuesta.result.serverInfo.name -eq 'visor') $respuesta
    $mcp.StandardInput.Close()
    $termino = $mcp.WaitForExit(10000)
    Comprobar 'visor mcp termina al cerrar su entrada' ($termino -and $mcp.ExitCode -eq 0) @{ termino = $termino }
    if (-not $termino) { taskkill /T /F /PID $mcp.Id | Out-Null }

    # 2. Una bóveda, una nota y una página HTML.
    $r = Visor @('open', $boveda)
    Comprobar 'abre la carpeta como bóveda' ($r.ok -eq $true) $r
    $r = Visor @('open', (Join-Path $boveda 'nota.md'))
    Comprobar 'abre la nota' ($r.ok -eq $true -and $r.loaded -eq $true -and "$($r.file)" -like '*nota.md') $r
    $r = Visor @('open', (Join-Path $boveda 'pagina.html'), '--tab')
    Comprobar 'abre la página HTML en otra pestaña' ($r.ok -eq $true -and $r.loaded -eq $true -and "$($r.file)" -like '*pagina.html') $r
    $r = Visor @('eval', 'document.title')
    Comprobar 'eval en una página de la bóveda' ($r.ok -eq $true -and $r.value -eq 'Página') $r
    $png = Join-Path $Salida 'pagina.png'
    $r = Visor @('screenshot', $png)
    Comprobar 'screenshot de la página' ($r.ok -eq $true -and $r.bytes -gt 0 -and (Test-Path $png)) $r
    Start-Sleep -Seconds 1
    Captura (Join-Path $Salida 'ventana-nota-y-pagina.png')

    # Entrada real: la zona del marco deja pasar el clic a la página, y el marco
    # recibe los suyos; los atajos llegan desde la página con el foco.
    Clic $proceso 600 400
    $r = Visor @('eval', 'document.documentElement.dataset.clics || "0"')
    Comprobar 'un clic en el hueco llega a la página' ($r.ok -eq $true -and $r.value -eq '1') $r
    $antes = Pestanas (Visor @('status'))
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.SendKeys]::SendWait('^t')
    Start-Sleep -Seconds 1
    $r = Visor @('status')
    Comprobar 'Ctrl+T abre una pestaña' ((Pestanas $r) -eq $antes + 1) @{ antes = $antes; despues = (Pestanas $r) }
    Clic $proceso 71 131
    $r = Visor @('status')
    Comprobar 'un clic en el panel abre la nota' ("$($r.file)" -like '*nota.md') $r

    # 3. Página hostil de la bóveda.
    # cargo test deja eventos en el registro de esta cuenta: cuentan solo los nuevos.
    $eventosAntes = (Visor @('status')).last_event_id
    $antes = Pestanas (Visor @('status'))
    $r = Visor @('open', (Join-Path $boveda 'hostil.html'), '--tab')
    Comprobar 'abre la página hostil' ($r.ok -eq $true) $r
    Start-Sleep -Seconds 3
    $despues = Visor @('status')
    $titulo = Visor @('eval', 'document.title')
    Comprobar 'la página hostil corrió su script' ($titulo.value -eq 'hostil listo') $titulo
    Comprobar 'la página hostil no abre pestañas' ((Pestanas $despues) -eq $antes + 1) @{ antes = $antes; despues = (Pestanas $despues) }
    $creadas = @(Get-ChildItem $boveda -Filter '*.md' | Where-Object Name -ne 'nota.md' | ForEach-Object Name)
    Comprobar 'la página hostil no crea notas' ($creadas.Count -eq 0) $creadas
    Comprobar 'la página hostil no escribe la nota' ((Get-Content -Raw (Join-Path $boveda 'nota.md')) -eq $notaAntes) $null

    # 4. Una web: otro origen, no confiable.
    $servidor = Start-Process -FilePath python -ArgumentList @('-m', 'http.server', '8765', '--bind', '127.0.0.1', '--directory', $web) -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds 2
    $antes = Pestanas (Visor @('status'))
    $r = Visor @('open', 'http://127.0.0.1:8765/sitio.html', '--tab')
    Comprobar 'abre la web' ($r.ok -eq $true -and "$($r.url)" -like '*sitio.html') $r
    Start-Sleep -Seconds 3
    $r = Visor @('eval', 'document.title')
    Comprobar 'eval rechazado en una web' ($r.ok -eq $false) $r
    $r = Visor @('screenshot', (Join-Path $Salida 'web.png'))
    Comprobar 'screenshot rechazado en una web' ($r.ok -eq $false -and -not (Test-Path (Join-Path $Salida 'web.png'))) $r
    $despues = Visor @('status')
    Comprobar 'la web no abre pestañas' ((Pestanas $despues) -eq $antes + 1) @{ antes = $antes; despues = (Pestanas $despues) }
    $creadas = @(Get-ChildItem $boveda -Filter '*.md' | Where-Object Name -ne 'nota.md' | ForEach-Object Name)
    Comprobar 'la web no crea notas' ($creadas.Count -eq 0) $creadas
    Comprobar 'la web no escribe la nota' ((Get-Content -Raw (Join-Path $boveda 'nota.md')) -eq $notaAntes) $null
    $eventos = Visor @('events', '--after', "$eventosAntes")
    Comprobar 'ni la hostil ni la web dejan eventos' ($eventos.ok -eq $true -and @($eventos.events).Count -eq 0) $eventos
    $r = Visor @('open', (Join-Path $boveda 'nota.md'))
    Comprobar 'una página de la bóveda carga sin errores de visor' ($r.ok -eq $true -and @($r.problems).Count -eq 0) $r
    Captura (Join-Path $Salida 'ventana-web.png')

    # 5. Cierre limpio: la ventana recibe WM_CLOSE, como al pulsar la X.
    $null = $proceso.CloseMainWindow()
    $cerro = $proceso.WaitForExit(20000)
    Comprobar 'cierra limpio' ($cerro -and $proceso.ExitCode -eq 0) @{ termino = $cerro; codigo = $(if ($cerro) { $proceso.ExitCode } else { $null }) }

    # 6. visorw.exe sin visor.exe al lado: un aviso de Windows, y no arranca nada.
    # El aviso espera a la persona, así que se lee su título y se cierra a la fuerza.
    if ($ExeW) {
        $sola = Join-Path $tmp 'sin-visor'
        New-Item -ItemType Directory -Force -Path $sola | Out-Null
        Copy-Item $ExeW $sola
        $huerfano = Start-Process -FilePath (Join-Path $sola 'visorw.exe') -PassThru
        Start-Sleep -Seconds 3
        $huerfano.Refresh()
        $dato = @{ termino = $huerfano.HasExited; titulo = $huerfano.MainWindowTitle; codigo = $(if ($huerfano.HasExited) { $huerfano.ExitCode } else { $null }) }
        $avisa = (-not $huerfano.HasExited -and $huerfano.MainWindowTitle -eq 'visor') -or ($huerfano.HasExited -and $huerfano.ExitCode -ne 0)
        Comprobar 'visorw.exe sin visor.exe avisa y no termina bien' $avisa $dato
        Captura (Join-Path $Salida 'visorw-sin-visor.png')
        if (-not $huerfano.HasExited) { taskkill /F /PID $huerfano.Id | Out-Null }

        # 7. Una actualización fallida a medias deja solo visor.exe.old: visorw.exe lo devuelve a su sitio.
        # `help` solo escribe la ayuda y termina, sin abrir la app.
        $rota = Join-Path $tmp 'actualizacion-rota'
        New-Item -ItemType Directory -Force -Path $rota | Out-Null
        Copy-Item $ExeW $rota
        Copy-Item $Exe (Join-Path $rota 'visor.exe.old')
        $arreglo = Start-Process -FilePath (Join-Path $rota 'visorw.exe') -ArgumentList 'help' -PassThru -Wait
        $dato = @{ codigo = $arreglo.ExitCode; archivos = @(Get-ChildItem $rota | ForEach-Object Name) }
        $vuelve = (Test-Path (Join-Path $rota 'visor.exe')) -and -not (Test-Path (Join-Path $rota 'visor.exe.old'))
        Comprobar 'visorw.exe devuelve visor.exe.old a su sitio' $vuelve $dato
    }
}
catch {
    Comprobar 'sin excepciones' $false "$_"
}
finally {
    if ($proceso -and -not $proceso.HasExited) {
        try { Captura (Join-Path $Salida 'ventana-al-fallar.png') } catch {}
        taskkill /T /F /PID $proceso.Id | Out-Null
    }
    if ($servidor -and -not $servidor.HasExited) { Stop-Process -Id $servidor.Id -Force }
}

$fallos = @($comprobaciones | Where-Object { -not $_.ok })
$informe = [pscustomobject]@{ ok = ($fallos.Count -eq 0); comprobaciones = $comprobaciones }
$informe | ConvertTo-Json -Depth 8 | Set-Content -Path (Join-Path $Salida 'informe.json') -Encoding utf8
$lineas = @('| comprobación | resultado |', '|---|---|')
foreach ($c in $comprobaciones) { $lineas += "| $($c.nombre) | $(if ($c.ok) { 'ok' } else { '**fallo**' }) |" }
$lineas -join "`n" | Set-Content -Path (Join-Path $Salida 'resumen.md') -Encoding utf8
if ($fallos.Count -gt 0) { exit 1 }
exit 0
