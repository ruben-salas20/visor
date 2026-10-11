// Reads and writes Obsidian's Excalidraw drawings (.excalidraw.md). The scene is
// a JSON block after "## Drawing", plain or compressed, and "## Text Elements"
// lists the texts with their 8-character ids. The sections around them are kept
// as they are. No DOM: tests in tools/excalidraw/test. `lz` is LZString, with
// compressToBase64 and decompressFromBase64.
(() => {
  "use strict";

  const DRAWING = /^##? Drawing[ \t]*\r?$/m;
  const FENCE_OPEN = /^```(json|compressed-json)[ \t]*\r?\n/m;
  const FENCE_CLOSE = /^```[ \t]*\r?$/m;
  // Older files have "# Text Elements"; the plugin reads both.
  const TEXT_HEAD = /^##? Text Elements[ \t]*\r?\n/m;
  const EMBEDDED_HEAD = /^## Embedded Files[ \t]*\r?$/m;
  const EMBED_LINE = /^([\w-]+):[ \t]*!?\[\[([^\]\r\n]+)]]/gm;

  // The index of the first match of `re` at or after `from`, or -1.
  function next(text, from, re) {
    const m = re.exec(text.slice(from));
    return m ? from + m.index : -1;
  }

  // Where the drawing block is: the fence lines and the body between them.
  function locate(source) {
    const heading = DRAWING.exec(source);
    if (!heading) throw new Error("El archivo no tiene la sección ## Drawing.");
    const from = heading.index + heading[0].length;
    const open = FENCE_OPEN.exec(source.slice(from));
    if (!open) throw new Error("La sección ## Drawing no tiene su bloque de JSON.");
    const start = from + open.index;
    const bodyStart = start + open[0].length;
    const close = FENCE_CLOSE.exec(source.slice(bodyStart));
    if (!close) throw new Error("El bloque del dibujo no se cierra.");
    const closeLine = bodyStart + close.index;
    return {
      heading: heading.index,
      open: start,
      close: closeLine + close[0].length,
      compressed: open[1] === "compressed-json",
      body: source.slice(bodyStart, closeLine).replace(/\r?\n$/, ""),
    };
  }

  function compressBlock(json, lz) {
    const packed = lz.compressToBase64(json);
    const parts = [];
    for (let i = 0; i < packed.length; i += 256) parts.push(packed.slice(i, i + 256));
    return parts.join("\n\n");
  }

  function decompressBlock(block, lz) {
    const json = lz.decompressFromBase64(block.replace(/[\r\n]/g, ""));
    if (!json) throw new Error("El bloque comprimido no se puede descomprimir.");
    return json;
  }

  // Obsidian tolerates text after the JSON, so the scene ends at its last brace.
  function parseScene(json) {
    let scene;
    try {
      scene = JSON.parse(json);
    } catch {
      try {
        scene = JSON.parse(json.slice(0, json.lastIndexOf("}") + 1));
      } catch {
        throw new Error("El JSON del dibujo no se puede leer.");
      }
    }
    if (!scene || !Array.isArray(scene.elements)) throw new Error("El dibujo no tiene elementos.");
    return scene;
  }

  // The image links of "## Embedded Files": `<fileId>: [[name]]`.
  function readEmbeds(source) {
    const heading = EMBEDDED_HEAD.exec(source);
    if (!heading) return [];
    const start = heading.index + heading[0].length;
    const end = next(source, start, /^(#{1,2} |%%)/m);
    const section = source.slice(start, end < 0 ? undefined : end);
    return [...section.matchAll(EMBED_LINE)].map((m) => ({ id: m[1], name: m[2].split(/[|#]/)[0].trim() }));
  }

  // Text ids in the file are exactly 8 characters. Longer ones (what Excalidraw
  // makes) get a short id derived from the old one, and every reference follows.
  const isShort = (id) => id.length === 8;

  function fnv(text) {
    let h = 0x811c9dc5;
    for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0;
    return h;
  }

  function shortId(id, taken) {
    for (let salt = ""; ; salt += "#") {
      const s = id + salt;
      const code = (fnv(s).toString(36) + fnv(s + "\u0001").toString(36)).padEnd(8, "0").slice(0, 8);
      if (!taken.has(code)) return code;
    }
  }

  function shortTextIds(elements) {
    const taken = new Set(elements.map((e) => e.id).filter(isShort));
    const map = new Map();
    for (const e of elements) {
      if (e.type !== "text" || isShort(e.id)) continue;
      const code = shortId(e.id, taken);
      taken.add(code);
      map.set(e.id, code);
    }
    if (!map.size) return elements;
    const rename = (id) => map.get(id) ?? id;
    return elements.map((e) => {
      const copy = { ...e, id: rename(e.id) };
      if (e.containerId) copy.containerId = rename(e.containerId);
      if (e.boundElements) copy.boundElements = e.boundElements.map((b) => (b ? { ...b, id: rename(b.id) } : b));
      return copy;
    });
  }

  // The text as the file keeps it. Obsidian's raw markdown (with its [[links]])
  // stays while visor has not changed the text; otherwise the new text goes in.
  function fileText(e, before) {
    if (before && before.rawText !== undefined && before.text === e.text) return before.rawText;
    return e.originalText ?? e.text;
  }

  // What the file says about a drawing, as read: the scene, whether it is
  // compressed, the images it links, and the texts as loaded (see writeMd).
  function readMd(source, lz) {
    const d = locate(source);
    const json = d.compressed ? decompressBlock(d.body, lz) : d.body;
    const scene = parseScene(json);
    const original = {};
    for (const e of scene.elements) {
      if (e.type === "text") original[e.id] = { text: e.text, rawText: e.rawText };
    }
    return { scene, compressed: d.compressed, embeds: readEmbeds(source), original };
  }

  // The new file: `scene` is what Excalidraw serializes (type, version, source,
  // elements, appState). The drawing block and "## Text Elements" are rewritten
  // from it; the frontmatter, the notice, "## Element Links", "## Embedded Files"
  // and any other text stay as they are. `files` goes empty: images live in the vault.
  function writeMd(source, scene, lz, original = {}) {
    const d = locate(source);
    const elements = shortTextIds(scene.elements);
    const texts = [];
    const out = elements.map((el, i) => {
      const before = scene.elements[i];
      if (before.type !== "text") return el;
      const keep = original[before.id];
      if (before.isDeleted) return el;
      const line = fileText(before, keep);
      texts.push(`${line} ^${el.id}\n\n`);
      return keep && keep.rawText !== undefined ? { ...el, rawText: line } : el;
    });
    const json = JSON.stringify({ ...scene, elements: out, files: {} }, null, "\t");
    const block = d.compressed ? compressBlock(json, lz) : json;
    const fence = "```" + (d.compressed ? "compressed-json" : "json") + "\n" + block + "\n```";

    const head = source.slice(0, d.heading);
    const section = texts.join("");
    const text = TEXT_HEAD.exec(head);
    let updated;
    if (text) {
      const start = text.index + text[0].length;
      // The section ends at the next heading or at the "%%" that opens the drawing.
      const end = next(head, start, /^(#{1,2} |%%)/m);
      updated = head.slice(0, start) + section + head.slice(end < 0 ? head.length : end);
    } else if (section) {
      // A file without the section (made by hand): it goes before the drawing.
      const at = head.endsWith("%%\n") ? head.length - 3 : head.length;
      const title = /^# Excalidraw Data/m.test(head) ? "" : "# Excalidraw Data\n\n";
      updated = head.slice(0, at) + title + "## Text Elements\n" + section + head.slice(at);
    } else {
      updated = head;
    }
    return updated + source.slice(d.heading, d.open) + fence + source.slice(d.close);
  }

  // A drawing of excalidraw.com: the JSON itself.
  function readJson(text) {
    return parseScene(text);
  }

  const api = { readMd, writeMd, readJson, compressBlock, decompressBlock, shortTextIds };
  if (typeof module !== "undefined") module.exports = api;
  else window.__visorDrawingFormat = api;
})();
