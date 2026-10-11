// Reads and writes Obsidian's Excalidraw drawings (.excalidraw.md). The scene is
// a JSON block after "## Drawing", plain or compressed. "## Text Elements" lists
// the texts and "## Element Links" the links of shapes, both by 8-character id.
// The other sections around them are kept as they are. No DOM: tests in
// tools/excalidraw/test. `lz` is LZString, with
// compressToBase64 and decompressFromBase64.
(() => {
  "use strict";

  const DRAWING = /^##? Drawing[ \t]*\r?$/m;
  const FENCE_OPEN = /^```(json|compressed-json)[ \t]*\r?\n/m;
  const FENCE_CLOSE = /^```[ \t]*\r?$/m;
  // Older files have "# Text Elements"; the plugin reads both.
  const TEXT_HEAD = /^##? Text Elements[ \t]*\r?\n/m;
  const EMBEDDED_HEAD = /^## Embedded Files[ \t]*\r?\n/m;

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

  // The lines of "## Embedded Files", one per file. A vault image is `<fileId>: [[name]]`
  // (it may end with ` {json colorMap}`); a URL is `<fileId>: https://…` (or http, ftp,
  // ftps); LaTeX is `<fileId>: $$…$$`. Only vault images are loaded (drawing.js).
  const EMBED_KINDS = [
    ["vault", /^([\w-]+):[ \t]*!?\[\[([^\]\r\n]+)]]/],
    ["url", /^([\w-]+):[ \t]*(?:https?|ftps?):\/\/\S/],
    ["latex", /^([\w-]+):[ \t]*\$\$/],
  ];

  // The entry a line of Embedded Files holds, or null for any other line.
  function embedOf(line) {
    for (const [kind, re] of EMBED_KINDS) {
      const m = re.exec(line);
      if (!m) continue;
      return kind === "vault" ? { kind, id: m[1], name: m[2].split(/[|#]/)[0].trim() } : { kind, id: m[1] };
    }
    return null;
  }

  // The entries of "## Embedded Files": `{ kind, id }`, and `name` for vault images.
  function readEmbeds(source) {
    const section = sectionAt(source, EMBEDDED_HEAD);
    if (!section) return [];
    return source.slice(section.start, section.stop).split(/\r?\n/).map(embedOf).filter(Boolean);
  }

  // Texts and linked shapes are listed by their 8-character id in the file (the
  // plugin does the same). Longer ids (what Excalidraw makes) get a short id derived
  // from the old one, and every reference follows.
  const isShort = (id) => id.length === 8;
  const needsShort = (e) => !isShort(e.id) && (e.type === "text" || e.link);

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

  function shortIds(elements) {
    const taken = new Set(elements.map((e) => e.id).filter(isShort));
    const map = new Map();
    for (const e of elements) {
      if (!needsShort(e)) continue;
      const code = shortId(e.id, taken);
      taken.add(code);
      map.set(e.id, code);
    }
    if (!map.size) return elements;
    const rename = (id) => map.get(id) ?? id;
    return elements.map((e) => {
      const copy = { ...e, id: rename(e.id) };
      if (e.containerId) copy.containerId = rename(e.containerId);
      if (e.frameId) copy.frameId = rename(e.frameId);
      for (const side of ["startBinding", "endBinding"]) {
        if (e[side]) copy[side] = { ...e[side], elementId: rename(e[side].elementId) };
      }
      if (e.boundElements) copy.boundElements = e.boundElements.map((b) => (b ? { ...b, id: rename(b.id) } : b));
      return copy;
    });
  }

  const LINKS_HEAD = /^##? Element Links[ \t]*\r?\n/m;

  // The `<8-character id>: <link>` lines of Element Links, as the plugin reads them.
  function parseLinks(body) {
    return new Map([...body.matchAll(/^([^\s:]{8}):[ \t]*(.*?)[ \t]*\r?$/gm)].map((m) => [m[1], m[2]]));
  }

  const sameLinks = (a, b) => a.size === b.size && [...a].every(([id, link]) => b.get(id) === link);

  // The text as the file keeps it. Obsidian's raw markdown (with its [[links]])
  // stays while visor has not changed the text; otherwise the new text goes in.
  function fileText(e, before) {
    if (before && before.rawText !== undefined && before.text === e.text) return before.rawText;
    return e.originalText ?? e.text;
  }

  // A "##" section runs from its heading line to the next heading or "%%" (the
  // comment that opens the drawing). `start` is the body, `stop` its end.
  const BOUNDARY = /^(#{1,2} |%%)/m;

  function sectionAt(text, heading) {
    const m = heading.exec(text);
    if (!m) return null;
    const start = m.index + m[0].length;
    const end = next(text, start, BOUNDARY);
    return { from: m.index, start, stop: end < 0 ? text.length : end };
  }

  // Lines of "## Embedded Files" whose image the scene no longer has go, with the blank
  // line after them. A section left empty goes with its heading, as the plugin does.
  // Only the file's text changes: the image files stay in the vault.
  function pruneEmbeds(head, scene) {
    const section = sectionAt(head, EMBEDDED_HEAD);
    if (!section) return head;
    const shown = new Set(scene.elements.filter((e) => e.type === "image" && !e.isDeleted).map((e) => e.fileId));
    const kept = [];
    let dropped = false;
    let blankAfter = false;
    // Lines with their newlines (no lookbehind: older WebKitGTK lacks it).
    for (const line of head.slice(section.start, section.stop).match(/[^\n]*\n|[^\n]+$/g) ?? []) {
      const e = embedOf(line);
      if (e && !shown.has(e.id)) {
        dropped = blankAfter = true;
        continue;
      }
      if (blankAfter && /^[ \t]*\r?\n$/.test(line)) continue;
      blankAfter = false;
      kept.push(line);
    }
    if (!dropped) return head;
    const body = kept.join("");
    if (!body.trim()) return head.slice(0, section.from) + head.slice(section.stop);
    return head.slice(0, section.start) + body + head.slice(section.stop);
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
  // elements, appState). The drawing block, "## Text Elements" and "## Element Links"
  // are rewritten from it; "## Embedded Files" loses the lines of deleted images; the
  // frontmatter, the notice and any other text stay as they are. `files` goes empty:
  // images live in the vault.
  function writeMd(source, scene, lz, original = {}) {
    const d = locate(source);
    const elements = shortIds(scene.elements);
    const texts = [];
    const shapeLinks = [];
    const textLinks = [];
    const out = elements.map((el, i) => {
      const before = scene.elements[i];
      if (!before.isDeleted && el.link) (before.type === "text" ? textLinks : shapeLinks).push([el.id, el.link]);
      if (before.type !== "text") return el;
      const keep = original[before.id];
      if (before.isDeleted) return el;
      const line = fileText(before, keep);
      // An empty text has nothing to point at; its element stays in the JSON.
      if (line.trim()) texts.push(`${line} ^${el.id}\n\n`);
      return keep && keep.rawText !== undefined ? { ...el, rawText: line } : el;
    });
    const links = [...shapeLinks, ...textLinks];
    const linkBody = links.map(([id, link]) => `${id}: ${link}\n\n`).join("");
    const json = JSON.stringify({ ...scene, elements: out, files: {} }, null, "\t");
    const block = d.compressed ? compressBlock(json, lz) : json;
    const fence = "```" + (d.compressed ? "compressed-json" : "json") + "\n" + block + "\n```";

    const head = pruneEmbeds(source.slice(0, d.heading), scene);
    const section = texts.join("");
    let updated = head;
    const text = sectionAt(head, TEXT_HEAD);
    if (text) {
      updated = head.slice(0, text.start) + section + head.slice(text.stop);
    } else if (section || linkBody) {
      // A file without the section (made by hand): it goes before the drawing.
      const at = head.endsWith("%%\n") ? head.length - 3 : head.length;
      const title = /^# Excalidraw Data/m.test(head) ? "" : "# Excalidraw Data\n\n";
      updated = head.slice(0, at) + title + "## Text Elements\n" + section + head.slice(at);
    }
    // Element Links goes right after the texts. Without links the heading goes too,
    // as the plugin does. An unchanged list keeps the file's own lines and order.
    const linked = sectionAt(updated, LINKS_HEAD);
    if (!linked) {
      if (linkBody) {
        const after = sectionAt(updated, TEXT_HEAD).stop;
        updated = updated.slice(0, after) + "## Element Links\n" + linkBody + updated.slice(after);
      }
    } else if (!linkBody) {
      updated = updated.slice(0, linked.from) + updated.slice(linked.stop);
    } else if (!sameLinks(parseLinks(updated.slice(linked.start, linked.stop)), new Map(links))) {
      updated = updated.slice(0, linked.start) + linkBody + updated.slice(linked.stop);
    }
    return updated + source.slice(d.heading, d.open) + fence + source.slice(d.close);
  }

  // A drawing of excalidraw.com: the JSON itself.
  function readJson(text) {
    return parseScene(text);
  }

  const api = { readMd, writeMd, readJson, compressBlock, decompressBlock, shortIds };
  if (typeof module !== "undefined") module.exports = api;
  else window.__visorDrawingFormat = api;
})();
