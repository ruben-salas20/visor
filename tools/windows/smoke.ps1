# Prueba de humo del visor real en Windows (CI, .github/workflows/windows-host.yml).
#
# Abre visor con su interfaz sobre una bóveda pequeña, en una instancia aislada
# (VISOR_SOCKET propio), y comprueba por la CLI: que responde, que abre una nota
# y una página HTML, que eval y screenshot funcionan en una página de la bóveda,
# que una página hostil de la bóveda y una web no consiguen nada, y que la ventana
# se cierra limpia. Deja en $Salida el informe (informe.json) y las capturas.
#
# Uso: smoke.ps1 -Exe <visor.exe> -Salida <carpeta>

param(
    [Parameter(Mandatory)] [string] $Exe,
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
try {
    $proceso = Start-Process -FilePath $Exe -PassThru -RedirectStandardOutput (Join-Path $Salida 'visor.stdout.txt') -RedirectStandardError (Join-Path $Salida 'visor.stderr.txt')

    # 1. La instancia responde.
    $estado = $null
    $limite = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $limite -and -not $proceso.HasExited) {
        $estado = Visor @('status')
        if ($estado.ok -eq $true) { break }
        Start-Sleep -Milliseconds 500
    }
    Comprobar 'visor status responde' ($estado.ok -eq $true) $estado
    if ($estado.ok -ne $true) { throw 'visor no respondió en 60 s' }

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
