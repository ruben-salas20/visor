// Builds assets/pdfjs/: PDF.js 6.4.299 for the PDF viewer, with only the files
// the viewer loads (see README.md). Run it with «npm run build». build.rs packs
// the result into the binary.
//
// Two sources of the same version: pdfjs-dist (npm) gives the minified library
// and worker; the full viewer (web/viewer.*) is only in the release zip, which
// is downloaded once and checked against its SHA-256.

import * as esbuild from "esbuild";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const version = "6.4.299";
const zipUrl = `https://github.com/mozilla/pdf.js/releases/download/v${version}/pdfjs-${version}-dist.zip`;
const zipSha256 = "f2245eb7ef3f674c872fe568a177bfec15874f5c034613e55d8e307f93ebd86d";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "node_modules/pdfjs-dist");
const cache = join(here, "cache");
const release = join(cache, `pdfjs-${version}`);
const out = resolve(here, "../../assets/pdfjs");

// The viewer's own texts, in visor's two languages.
const locales = ["en-US", "es-ES"];

// Only these wasm decoders: JPEG 2000, JBIG2 and color management. The
// «nowasm» fallbacks and quickjs are left out (scripting is off).
const wasm = ["openjpeg.wasm", "jbig2.wasm", "qcms_bg.wasm"];
const wasmLicenses = ["LICENSE_JBIG2", "LICENSE_PDFJS_JBIG2", "LICENSE_OPENJPEG", "LICENSE_PDFJS_OPENJPEG", "LICENSE_QCMS", "LICENSE_PDFJS_QCMS"];

// Already minified by PDF.js: copied as they are.
const copied = [
  ["build/pdf.min.mjs", "pdf.min.mjs"],
  ["build/pdf.worker.min.mjs", "pdf.worker.min.mjs"],
];

// A CDN address in the output would fetch code from the web: forbidden.
const forbidden = /https?:\/\/(cdn\.|unpkg\.com|cdnjs|jsdelivr)/;

async function fetchRelease() {
  if (existsSync(join(release, "web/viewer.mjs"))) return;
  mkdirSync(cache, { recursive: true });
  const zip = join(cache, `pdfjs-${version}-dist.zip`);
  if (!existsSync(zip)) {
    const response = await fetch(zipUrl);
    if (!response.ok) throw new Error(`No se pudo descargar ${zipUrl}: ${response.status}`);
    writeFileSync(zip, Buffer.from(await response.arrayBuffer()));
  }
  const sum = createHash("sha256").update(readFileSync(zip)).digest("hex");
  if (sum !== zipSha256) {
    rmSync(zip);
    throw new Error(`SHA-256 de ${zip} no coincide: ${sum}`);
  }
  execFileSync("unzip", ["-q", "-o", zip, "web/*", "-d", release]);
}

await fetchRelease();
const web = join(release, "web");

const distVersion = JSON.parse(readFileSync(join(dist, "package.json"), "utf8")).version;
if (distVersion !== version) throw new Error(`pdfjs-dist ${distVersion} y el visor ${version} no coinciden`);

rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, "images"), { recursive: true });
mkdirSync(join(out, "wasm"), { recursive: true });

for (const [from, to] of copied) copyFileSync(join(dist, from), join(out, to));

// The viewer is not minified upstream: esbuild does it here. It reads
// `globalThis.pdfjsLib`, which the page sets first.
{
  const { code } = await esbuild.transform(readFileSync(join(web, "viewer.mjs"), "utf8"), { minify: true, format: "esm", legalComments: "none", target: "safari16" });
  writeFileSync(join(out, "viewer.mjs"), code);
}

const viewerCss = readFileSync(join(web, "viewer.css"), "utf8");
const css = await esbuild.transform(viewerCss, { loader: "css", minify: true, legalComments: "none" });
writeFileSync(join(out, "viewer.css"), css.code);

// The body of viewer.html: visor's page (src/vault/pdf.rs) writes the head and
// loads this into the body before the viewer starts.
{
  const html = readFileSync(join(web, "viewer.html"), "utf8");
  const body = html.match(/<body[^>]*>([\s\S]*)<\/body>/);
  if (!body) throw new Error("viewer.html sin <body>");
  const markup = body[1]
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/>\s+</g, "><")
    .trim();
  if (/<script/i.test(markup)) throw new Error("viewer.html trae scripts en el cuerpo");
  writeFileSync(join(out, "viewer-body.html"), markup);
}

// Images the stylesheet or the viewer's markup name; the rest of web/images is
// left out.
const images = new Set();
for (const text of [viewerCss, readFileSync(join(out, "viewer-body.html"), "utf8")]) {
  for (const m of text.matchAll(/images\/([\w.-]+)/g)) images.add(m[1]);
}
for (const image of [...images].sort()) {
  const path = join(web, "images", image);
  if (!existsSync(path)) throw new Error(`Imagen usada por el visor y no encontrada: ${image}`);
  copyFileSync(path, join(out, "images", image));
}

// locale.json lists only the languages that are packed: the viewer falls back to en-US.
{
  const all = JSON.parse(readFileSync(join(web, "locale/locale.json"), "utf8"));
  const kept = {};
  for (const [lang, file] of Object.entries(all)) {
    if (locales.some((l) => file.startsWith(`${l}/`))) kept[lang] = file;
  }
  for (const l of locales) {
    mkdirSync(join(out, "locale", l), { recursive: true });
    copyFileSync(join(web, "locale", l, "viewer.ftl"), join(out, "locale", l, "viewer.ftl"));
  }
  writeFileSync(join(out, "locale/locale.json"), JSON.stringify(kept));
}

for (const file of [...wasm, ...wasmLicenses]) {
  const path = join(dist, "wasm", file);
  if (!existsSync(path)) throw new Error(`Falta ${file} en pdfjs-dist`);
  copyFileSync(path, join(out, "wasm", file));
}

const walk = (dir) => readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]));
const files = walk(out).map((p) => relative(out, p).split(sep).join("/")).sort();
for (const f of files) {
  if (/\.(m?js|css|html)$/.test(f) && forbidden.test(readFileSync(join(out, f), "utf8"))) throw new Error(`${f}: contiene una dirección de CDN`);
}

const size = (path) => statSync(path).size;
for (const f of files) console.log(`${size(join(out, f)).toString().padStart(10)}  ${f}`);
console.log(`${"".padStart(10, " ")}  ${files.length} ficheros, ${files.reduce((n, f) => n + size(join(out, f)), 0)} bytes`);
