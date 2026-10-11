// Builds assets/excalidraw/: one IIFE with Excalidraw, React and lz-string
// (window.Excalidraw), its stylesheet and the woff2 fonts it loads.
// Run it with «npm run build». build.rs packs the result into the binary.

import * as esbuild from "esbuild";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const lib = join(here, "node_modules/@excalidraw/excalidraw/dist/prod");
const out = resolve(here, "../../assets/excalidraw");

// Xiaolai alone is 13 MB of CJK subsets. Not copied: its faces are only
// requested when a page shows CJK text, which visor does not do for now.
const SKIPPED_FONTS = ["Xiaolai"];

// Only these two locales are bundled; the rest are empty modules. Visor shows
// Spanish, with English as Excalidraw's own fallback.
const KEPT_LOCALES = /^(en|es-ES)-/;

// The library falls back to esm.sh when a font file fails to load. Point that
// fallback at this same server instead, so no request ever leaves the machine.
const CDN = "https://esm.sh/";

const EMPTY_LOCALE = "visor-locale-vacio";

// The library subsets fonts on a worker when `Worker` exists. visor has no
// worker file, so that attempt always fails and logs an error. Reading the flag
// as false sends it straight to the main thread, which stub-subset.js serves.
const WORKER_FLAG = 'typeof Worker<"u"';
let workerFlagPatched = false;

// Settings of the excalidraw.com app that ship inside the library: its Firebase
// project (a public web key, which secret scanners flag), and its collaboration,
// AI and library servers. Visor uses none; emptied, nothing can reach them.
const APP_ENV = [
  ["VITE_APP_FIREBASE_CONFIG", "VITE_APP_FIREBASE_CONFIG:'[^']*'", "'{}'"],
  ["VITE_APP_AI_BACKEND", 'VITE_APP_AI_BACKEND:"[^"]*"', '""'],
  ["VITE_APP_WS_SERVER_URL", 'VITE_APP_WS_SERVER_URL:"[^"]*"', '""'],
  ["VITE_APP_LIBRARY_BACKEND", 'VITE_APP_LIBRARY_BACKEND:"[^"]*"', '""'],
];

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

const visor = {
  name: "visor",
  setup(build) {
    build.onResolve({ filter: /^@excalidraw\/mermaid-to-excalidraw$/ }, () => ({
      path: join(here, "stub-mermaid.js"),
    }));

    build.onResolve({ filter: /^\.\/subset-(shared|worker)\.chunk\.js$/ }, () => ({
      path: join(here, "stub-subset.js"),
    }));

    build.onResolve({ filter: /^\.\/locales\/.+\.js$/ }, (args) => {
      if (KEPT_LOCALES.test(basename(args.path))) return null;
      return { path: EMPTY_LOCALE, namespace: "visor" };
    });
    build.onLoad({ filter: /.*/, namespace: "visor" }, () => ({
      contents: "export default {};",
      loader: "js",
    }));

    build.onLoad({ filter: /dist[\\/]prod[\\/].*\.js$/ }, (args) => {
      let text = readFileSync(args.path, "utf8");
      const original = text;
      const count = text.split(CDN).length - 1;
      if (count > 1) throw new Error(`${args.path}: el CDN aparece ${count} veces, se esperaba 0 o 1`);
      // The fallback is a template literal: ${location.origin} sits inside it.
      if (count === 1) text = text.replace(CDN, "${location.origin}/");
      for (const [name, pattern, empty] of APP_ENV) {
        const found = text.match(new RegExp(pattern, "g"))?.length ?? 0;
        if (found > 1) throw new Error(`${args.path}: ${name} aparece ${found} veces, se esperaba 0 o 1`);
        if (found === 1) text = text.replace(new RegExp(pattern), `${name}:${empty}`);
      }
      const flags = text.split(WORKER_FLAG).length - 1;
      if (flags > 1) throw new Error(`${args.path}: la comprobación de Worker aparece ${flags} veces, se esperaba 0 o 1`);
      if (flags === 1) {
        text = text.replace(WORKER_FLAG, "!1");
        workerFlagPatched = true;
      }
      return text === original ? null : { contents: text, loader: "js" };
    });
  },
};

const result = await esbuild.build({
  entryPoints: [join(here, "entry.js")],
  outfile: join(out, "excalidraw.min.js"),
  bundle: true,
  minify: true,
  format: "iife",
  platform: "browser",
  target: "safari16",
  conditions: ["production"],
  define: { "process.env.NODE_ENV": '"production"' },
  legalComments: "none",
  metafile: true,
  logLevel: "warning",
  plugins: [visor],
});

// If Excalidraw changes its code, the flag above is not found and the error
// would come back without any sign: fail the build instead.
if (!workerFlagPatched) throw new Error("No se encontró la comprobación de Worker de Excalidraw");

// Mermaid and KaTeX must not be in the bundle at all.
const forbidden = /node_modules[\\/](mermaid|katex|@excalidraw[\\/]mermaid-to-excalidraw)[\\/]|subset-(shared|worker)\.chunk/;
const bad = Object.keys(result.metafile.inputs).filter((path) => forbidden.test(path));
if (bad.length) throw new Error(`Entran en el paquete: ${bad.join(", ")}`);

// No key of any Google service may reach the repository.
if (/AIza[0-9A-Za-z_-]{35}/.test(readFileSync(join(out, "excalidraw.min.js"), "utf8")))
  throw new Error("Queda una clave de Google en el paquete");

// Fonts. Excalidraw asks for "./fonts/<Familia>/<archivo>.woff2" relative to
// EXCALIDRAW_ASSET_PATH, and the stylesheet asks for the same paths relative to
// itself, so both resolve to one tree.
const bundle = readFileSync(join(out, "excalidraw.min.js"), "utf8");
const css = readFileSync(join(lib, "index.css"), "utf8");
writeFileSync(join(out, "excalidraw.css"), css);

const copied = new Set();
for (const family of readdirSync(join(lib, "fonts"))) {
  if (SKIPPED_FONTS.includes(family)) continue;
  for (const file of readdirSync(join(lib, "fonts", family))) {
    if (!file.endsWith(".woff2")) continue;
    cpSync(join(lib, "fonts", family, file), join(out, "fonts", family, file), { recursive: true });
    copied.add(`${family}/${file}`);
  }
}

const referenced = new Set([...`${bundle}${css}`.matchAll(/\.\/fonts\/([^"'\)\s]+\.woff2)/g)].map((m) => m[1]));
const missing = [...referenced].filter((f) => !copied.has(f) && !SKIPPED_FONTS.includes(f.split("/")[0]));
if (missing.length) throw new Error(`Fuentes referenciadas y no copiadas: ${missing.join(", ")}`);

const size = (path) => statSync(path).size;
const walk = (dir) => readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]));
const files = walk(out).map((p) => relative(out, p).split(sep).join("/")).sort();
for (const f of files) console.log(`${size(join(out, f)).toString().padStart(10)}  ${f}`);
console.log(`${"".padStart(10, " ")}  ${files.length} ficheros, ${files.reduce((n, f) => n + size(join(out, f)), 0)} bytes`);
if (!existsSync(join(out, "excalidraw.min.js"))) throw new Error("No se generó el paquete");
