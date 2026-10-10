#!/usr/bin/env node
// Records tools/video/intro.html frame by frame and turns it into a GIF with ffmpeg.
// No npm dependencies: plain Chrome DevTools Protocol over Node's built-in WebSocket.
//
//   node tools/video/grabar.mjs [--lang es|en|todos] [--logo FILE] [--datos FILE]
//                               [--fps 15] [--salida docs/img] [--cdp ws://...] [--png DIR]
//
// Time never runs on its own: for each frame the page gets intro.render(i / fps) and then
// a screenshot, so the result is the same on any machine and at any speed.
//
// By default it launches its own headless Chromium (no window, temporary profile) because
// Obscura's renderer drops gradients and antialiasing and blocks 127.0.0.1 without
// --allow-private-network. --cdp uses an already running browser instead.

import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const ANCHO = 960, ALTO = 540;

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, all) => {
    if (a.startsWith("--")) acc.push([a.slice(2), all[i + 1]?.startsWith("--") ? true : all[i + 1] ?? true]);
    return acc;
  }, []),
);
const idiomas = !args.lang || args.lang === "todos" ? ["es", "en"] : [args.lang];
const fps = Number(args.fps || 15);
const salida = resolve(args.salida || join(RAIZ, "docs/img"));
const externos = {}; // /_ext/<name> -> file outside the repo
for (const k of ["logo", "datos"]) if (args[k]) externos[k] = resolve(args[k]);

// --- Static server: the repo, plus the files given by --logo and --datos.
const TIPOS = { ".html": "text/html; charset=utf-8", ".json": "application/json", ".svg": "image/svg+xml",
  ".png": "image/png", ".js": "text/javascript", ".css": "text/css", ".woff2": "font/woff2" };
const servidor = createServer((req, res) => {
  const ruta = decodeURIComponent(new URL(req.url, "http://x").pathname);
  let archivo;
  const ext = ruta.match(/^\/_ext\/(logo|datos)$/);
  if (ext) archivo = externos[ext[1]];
  else {
    archivo = resolve(RAIZ, "." + ruta);
    if (!archivo.startsWith(RAIZ + sep)) archivo = null;
  }
  try {
    if (!archivo || !statSync(archivo).isFile()) throw 0;
    res.writeHead(200, { "content-type": TIPOS[extname(archivo)] || (ext?.[1] === "logo" ? "image/svg+xml" : "application/octet-stream") });
    res.end(readFileSync(archivo));
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((ok) => servidor.listen(0, "127.0.0.1", ok));
const base = `http://127.0.0.1:${servidor.address().port}`;

// --- Browser.
let navegador, perfil, wsUrl = args.cdp;
if (wsUrl && wsUrl.startsWith("http")) wsUrl = (await (await fetch(wsUrl + "/json/version")).json()).webSocketDebuggerUrl;
if (!wsUrl) {
  perfil = mkdtempSync(join(tmpdir(), "visor-intro-"));
  navegador = spawn(args.chromium || "chromium", [
    "--headless=new", "--remote-debugging-port=0", `--user-data-dir=${perfil}`, "--no-first-run",
    "--no-default-browser-check", "--hide-scrollbars", "--mute-audio", "--force-color-profile=srgb",
    "--font-render-hinting=none", "about:blank",
  ], { stdio: ["ignore", "ignore", "pipe"] });
  wsUrl = await new Promise((ok, mal) => {
    let buf = "";
    navegador.stderr.on("data", (d) => {
      buf += d;
      const m = buf.match(/DevTools listening on (ws:\/\/\S+)/);
      if (m) ok(m[1]);
    });
    navegador.on("exit", () => mal(new Error("Chromium se cerró al arrancar:\n" + buf)));
  });
}

const ws = new WebSocket(wsUrl);
await new Promise((ok, mal) => { ws.onopen = ok; ws.onerror = mal; });
let n = 0;
const pendientes = new Map();
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pendientes.has(m.id)) {
    const [ok, mal] = pendientes.get(m.id);
    pendientes.delete(m.id);
    m.error ? mal(new Error(JSON.stringify(m.error))) : ok(m.result);
  }
};
const cdp = (method, params = {}, sessionId) => new Promise((ok, mal) => {
  const id = ++n;
  pendientes.set(id, [ok, mal]);
  ws.send(JSON.stringify({ id, method, params, sessionId }));
});
const evaluar = async (expr, s) => {
  const r = await cdp("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true }, s);
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
  return r.result.value;
};

const carpetaPng = args.png ? resolve(args.png) : mkdtempSync(join(tmpdir(), "visor-intro-png-"));
mkdirSync(salida, { recursive: true });

try {
  for (const lang of idiomas) {
    const { browserContextId } = await cdp("Target.createBrowserContext");
    const { targetId } = await cdp("Target.createTarget", { url: "about:blank", browserContextId });
    const { sessionId: s } = await cdp("Target.attachToTarget", { targetId, flatten: true });
    try {
      await cdp("Emulation.setDeviceMetricsOverride", { width: ANCHO, height: ALTO, deviceScaleFactor: 1, mobile: false }, s);
      await cdp("Page.enable", {}, s);
      const p = new URLSearchParams({ lang, grabar: "1" });
      if (externos.logo) p.set("logo", "/_ext/logo");
      if (externos.datos) p.set("datos", "/_ext/datos");
      await cdp("Page.navigate", { url: `${base}/tools/video/intro.html?${p}` }, s);
      await evaluar("new Promise(r => { const w = () => window.intro ? r() : setTimeout(w, 20); w(); })", s);
      await evaluar("intro.listo", s);
      const total = Math.round((await evaluar("intro.DURACION", s)) * fps);

      const dir = join(carpetaPng, lang);
      rmSync(dir, { recursive: true, force: true });
      mkdirSync(dir, { recursive: true });
      for (let i = 0; i <= total; i++) {
        await evaluar(`intro.render(${i / fps}); new Promise(r => requestAnimationFrame(() => r()))`, s);
        const { data } = await cdp("Page.captureScreenshot", { format: "png" }, s);
        writeFileSync(join(dir, `f${String(i).padStart(4, "0")}.png`), Buffer.from(data, "base64"));
      }

      const gif = join(salida, `intro.${lang}.gif`);
      execFileSync("ffmpeg", [
        "-v", "error", "-y", "-framerate", String(fps), "-i", join(dir, "f%04d.png"),
        "-filter_complex",
        "split[a][b];[a]palettegen=max_colors=128:stats_mode=full[p];" +
        "[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle",
        "-loop", "0", gif,
      ], { stdio: "inherit" });
      console.log(`${gif}  ${total + 1} fotogramas  ${(statSync(gif).size / 1024).toFixed(0)} KB`);
    } finally {
      await cdp("Target.closeTarget", { targetId }).catch(() => {});
      await cdp("Target.disposeBrowserContext", { browserContextId }).catch(() => {});
    }
  }
} finally {
  ws.close();
  servidor.close();
  navegador?.kill();
  if (perfil) setTimeout(() => rmSync(perfil, { recursive: true, force: true }), 300);
  if (!args.png) rmSync(carpetaPng, { recursive: true, force: true });
}
