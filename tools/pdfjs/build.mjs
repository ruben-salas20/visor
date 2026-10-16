// Builds assets/pdfjs/: PDF.js 6.4.299 for the PDF viewer, with only the files
// the viewer loads (see README.md). Run it with «npm run build». build.rs packs
// the result into the binary.

import * as esbuild from "esbuild";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "node_modules/pdfjs-dist");
const out = resolve(here, "../../assets/pdfjs");

// Images the viewer's stylesheet uses. Others in web/images are for features
// visor does not show (annotation editors, sidebars with icons not used).
const cssImages = [...new Set([...readFileSync(join(dist, "web/pdf_viewer.css"), "utf8").matchAll(/images\/([\w.-]+)/g)].map((m) => m[1]))].sort();

// Only these wasm decoders: JPEG 2000, JBIG2 and color management. The
// «nowasm» fallbacks and quickjs are left out (scripting is off).
const wasm = ["openjpeg.wasm", "jbig2.wasm", "qcms_bg.wasm"];
const wasmLicenses = ["LICENSE_JBIG2", "LICENSE_PDFJS_JBIG2", "LICENSE_OPENJPEG", "LICENSE_PDFJS_OPENJPEG", "LICENSE_QCMS", "LICENSE_PDFJS_QCMS"];

// Already minified by PDF.js: copied as they are.
const copied = [
  ["build/pdf.min.mjs", "pdf.min.mjs"],
  ["build/pdf.worker.min.mjs", "pdf.worker.min.mjs"],
];

// The viewer is not minified upstream: esbuild does it here. It keeps the
// exports, and it reads `globalThis.pdfjsLib`, which the page sets first.
const minified = ["web/pdf_viewer.mjs"];

// A CDN address in the output would fetch code from the web: forbidden.
const forbidden = /https?:\/\/(cdn\.|unpkg\.com|cdnjs|jsdelivr)/;

rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, "images"), { recursive: true });
mkdirSync(join(out, "wasm"), { recursive: true });

for (const [from, to] of copied) copyFileSync(join(dist, from), join(out, to));

for (const from of minified) {
  const name = from.split("/").pop();
  const { code } = await esbuild.transform(readFileSync(join(dist, from), "utf8"), { minify: true, format: "esm", legalComments: "none", target: "safari16" });
  writeFileSync(join(out, name), code);
}

const css = await esbuild.transform(readFileSync(join(dist, "web/pdf_viewer.css"), "utf8"), { loader: "css", minify: true, legalComments: "none" });
writeFileSync(join(out, "pdf_viewer.css"), css.code);

for (const image of cssImages) {
  const path = join(dist, "web/images", image);
  if (!existsSync(path)) throw new Error(`Imagen usada por el CSS y no encontrada: ${image}`);
  copyFileSync(path, join(out, "images", image));
}

for (const file of [...wasm, ...wasmLicenses]) {
  const path = join(dist, "wasm", file);
  if (!existsSync(path)) throw new Error(`Falta ${file} en pdfjs-dist`);
  copyFileSync(path, join(out, "wasm", file));
}

const walk = (dir) => readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]));
const files = walk(out).map((p) => relative(out, p).split(sep).join("/")).sort();
for (const f of files) {
  if (/\.(m?js|css)$/.test(f) && forbidden.test(readFileSync(join(out, f), "utf8"))) throw new Error(`${f}: contiene una dirección de CDN`);
}

const size = (path) => statSync(path).size;
for (const f of files) console.log(`${size(join(out, f)).toString().padStart(10)}  ${f}`);
console.log(`${"".padStart(10, " ")}  ${files.length} ficheros, ${files.reduce((n, f) => n + size(join(out, f)), 0)} bytes`);
