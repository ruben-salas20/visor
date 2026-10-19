// Regenerates assets/codemirror/codemirror.min.js, as ADR 0028 says, with one
// patch on @codemirror/view applied while bundling.
//
// WebKitGTK 2.54 aborts the web process when a selection at the end of a
// contenteditable lands after a replaced element (<img>, <input>…) while an
// AT-SPI client watches (WebKit bug 279313): the DOM range counts the element
// as one character (U+FFFC) that the accessible text omits, so the caret offset
// lands one past the end of the UTF-16 -> UTF-8 mapping and is indexed out of
// bounds. Upstream fixes it by clamping the offset; until that ships, the
// elements visor puts in the editable must not be replaced. CodeMirror draws
// its widget buffer — a bare <img> around every widget — so it becomes a
// <span>: the same zero-width box next to uneditable content, which
// TextIterator does not count. visor's own widgets do the same in editor.js.
import { readFileSync } from "node:fs";
import { build } from "esbuild";

const VIEW = /[\\/]@codemirror[\\/]view[\\/]dist[\\/]index\.js$/;
const BUFFER = /let img = document\.createElement\("img"\);(\s+)img\.className = "cm-widgetBuffer";(\s+)img\.setAttribute\("aria-hidden", "true"\);(\s+)super\(img, 0, flags\);/;
const STYLE = /("\.cm-widgetBuffer": \{[\s\S]*?display: )"inline"/;

const widgetBuffer = {
  name: "widget-buffer-without-replaced-element",
  setup(build) {
    build.onLoad({ filter: VIEW }, (args) => {
      const source = readFileSync(args.path, "utf8");
      const buffer = source.match(BUFFER);
      if (!buffer) throw new Error("@codemirror/view changed: update the widget buffer patch in tools/codemirror/build.mjs");
      const span = `let span = document.createElement("span");${buffer[1]}span.className = "cm-widgetBuffer";${buffer[2]}span.setAttribute("aria-hidden", "true");${buffer[3]}super(span, 0, flags);`;
      let patched = source.slice(0, buffer.index) + span + source.slice(buffer.index + buffer[0].length);
      if (!STYLE.test(patched)) throw new Error("@codemirror/view changed: update the widget buffer style patch in tools/codemirror/build.mjs");
      // Inline, an empty <span> has no box; inline-block keeps the 1em-tall box the <img> had.
      patched = patched.replace(STYLE, '$1"inline-block"');
      return { contents: patched, loader: "js" };
    });
  },
};

await build({
  entryPoints: ["entry.js"],
  bundle: true,
  minify: true,
  format: "iife",
  globalName: "CM",
  legalComments: "none",
  target: "safari16",
  outfile: "../../assets/codemirror/codemirror.min.js",
  plugins: [widgetBuffer],
});
