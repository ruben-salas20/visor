// node --test tools/excalidraw/test/drawing-format.test.mjs
// Format of Obsidian's .excalidraw.md drawings (src/page/drawing-format.js).
// The fixtures come from the Obsidian plugin's source, see docs in scratchpad notes.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const format = require("../../../src/page/drawing-format.js");
const lz = require("lz-string");
const fixture = (name) => readFileSync(join(here, "fixtures", name), "utf8");

// What Excalidraw serializes: the same object the plugin writes, minus its tabs.
const scene = (text) => ({ type: "excalidraw", version: 2, source: "https://excalidraw.com", ...text });

test("reads the plain JSON block", () => {
  const doc = format.readMd(fixture("ejemplo-json-plano.excalidraw.md"), lz);
  assert.equal(doc.compressed, false);
  assert.ok(doc.scene.elements.length > 0);
  assert.deepEqual(doc.embeds, []);
});

test("reads the compressed block, in chunks separated by blank lines", () => {
  const source = fixture("ejemplo-compressed-json.excalidraw.md");
  const doc = format.readMd(source, lz);
  assert.equal(doc.compressed, true);
  assert.deepEqual(doc.scene.elements.filter((e) => e.type === "text").map((e) => e.text), ["Primera línea", "Segunda línea"]);
  assert.match(source, /\n\n/, "the fixture really is chunked");
});

test("lists the embedded images without the element links", () => {
  const doc = format.readMd(fixture("ejemplo-imagen-incrustada.excalidraw.md"), lz);
  assert.deepEqual(doc.embeds, [{ id: "f1e2d3c4", name: "imagen-ejemplo.png" }]);
});

for (const name of ["ejemplo-json-plano", "ejemplo-compressed-json", "ejemplo-imagen-incrustada"]) {
  test(`round trip without changes gives the same file: ${name}`, () => {
    const source = fixture(`${name}.excalidraw.md`);
    const doc = format.readMd(source, lz);
    assert.equal(format.writeMd(source, scene(doc.scene), lz, doc.original), source);
  });
}

test("a changed text rewrites the Text Elements section and the JSON, nothing else", () => {
  const source = fixture("ejemplo-json-plano.excalidraw.md");
  const doc = format.readMd(source, lz);
  const edited = structuredClone(doc.scene);
  edited.elements[0].text = "Adiós mundo";
  edited.elements[0].originalText = "Adiós mundo";
  const out = format.writeMd(source, edited, lz, doc.original);
  assert.match(out, /## Text Elements\nAdiós mundo \^a1b2c3d4\n\n## Element Links/);
  assert.equal(format.readMd(out, lz).scene.elements[0].text, "Adiós mundo");
  // Frontmatter, notice, Element Links and the rest of the file are untouched.
  assert.equal(out.slice(0, out.indexOf("# Excalidraw Data")), source.slice(0, source.indexOf("# Excalidraw Data")));
  assert.match(out, /## Element Links\nr1r2r3r4: \[\[Notas\]\]\n\n%%\n## Drawing\n```json\n/);
  assert.equal(out.slice(out.indexOf("```\n%%")), source.slice(source.indexOf("```\n%%")));
});

test("a Text Elements section commented with %% keeps its comment when a text changes", () => {
  const source = fixture("ejemplo-texto-comentado.excalidraw.md");
  const doc = format.readMd(source, lz);
  const edited = structuredClone(doc.scene);
  edited.elements[0].text = edited.elements[0].originalText = "Adiós";
  const out = format.writeMd(source, edited, lz, doc.original);
  assert.match(out, /^%%\n# Excalidraw Data\n\n## Text Elements\nAdiós \^a1b2c3d4\n\n## Element Links\n/m);
  assert.equal(out.split("%%").length, source.split("%%").length, "still one comment, no %% added or lost");
  assert.equal(format.readMd(out, lz).scene.elements[0].text, "Adiós");
});

test("a commented section with the older single-# heading is rewritten in place, not duplicated", () => {
  const source = fixture("ejemplo-texto-comentado-antiguo.excalidraw.md");
  const doc = format.readMd(source, lz);
  const edited = structuredClone(doc.scene);
  edited.elements[0].text = edited.elements[0].originalText = "Adiós";
  const out = format.writeMd(source, edited, lz, doc.original);
  assert.equal(out.match(/Text Elements/g).length, 1, "one Text Elements heading");
  assert.doesNotMatch(out, /Hola mundo/);
  assert.match(out, /# Text Elements\nAdiós \^a1b2c3d4\n\n## Element Links\n/);
  assert.equal(format.readMd(out, lz).scene.elements[0].text, "Adiós");
});

test("the raw markdown of an unchanged text (its [[links]]) is kept", () => {
  const source = fixture("ejemplo-json-plano.excalidraw.md");
  const doc = format.readMd(source, lz);
  const el = structuredClone(doc.scene.elements[0]);
  el.rawText = "Hola [[Notas]]";
  const withRaw = { ...doc.scene, elements: [el] };
  const original = { [el.id]: { text: el.text, rawText: el.rawText } };
  const out = format.writeMd(source, withRaw, lz, original);
  assert.match(out, /Hola \[\[Notas\]\] \^a1b2c3d4/);
  // Once the text changes, the new text goes in and the raw markup is dropped.
  el.text = el.originalText = "Adiós";
  const changed = format.writeMd(source, { ...doc.scene, elements: [el] }, lz, original);
  assert.match(changed, /Adiós \^a1b2c3d4/);
});

test("a text with an Excalidraw id gets an 8-character id, and its references follow", () => {
  const source = fixture("ejemplo-json-plano.excalidraw.md");
  const doc = format.readMd(source, lz);
  const container = { id: "rect0001", type: "rectangle", boundElements: [{ id: "texto-largo-de-excalidraw", type: "text" }] };
  const label = { id: "texto-largo-de-excalidraw", type: "text", containerId: "rect0001", text: "Nuevo", originalText: "Nuevo", isDeleted: false, rawText: undefined };
  const out = format.writeMd(source, { ...doc.scene, elements: [...doc.scene.elements, container, label] }, lz, doc.original);
  const section = out.slice(out.indexOf("## Text Elements"), out.indexOf("## Element Links"));
  const id = section.match(/Nuevo \^(\S{8})\n/)?.[1];
  assert.ok(id, "the new text is in the section with an 8-character id");
  const back = format.readMd(out, lz).scene.elements;
  const newLabel = back.find((e) => e.text === "Nuevo");
  assert.equal(newLabel.id, id);
  assert.equal(newLabel.containerId, "rect0001");
  assert.equal(back.find((e) => e.id === "rect0001").boundElements[0].id, id);
});

test("a link added to a shape goes to Element Links, after the links already there", () => {
  const source = fixture("ejemplo-json-plano.excalidraw.md");
  const doc = format.readMd(source, lz);
  const edited = structuredClone(doc.scene);
  edited.elements[0].link = "[[Otra]]";
  const out = format.writeMd(source, edited, lz, doc.original);
  assert.match(out, /## Element Links\nr1r2r3r4: \[\[Notas\]\]\n\na1b2c3d4: \[\[Otra\]\]\n\n%%\n## Drawing/);
});

test("a changed link is rewritten in Element Links", () => {
  const source = fixture("ejemplo-json-plano.excalidraw.md");
  const doc = format.readMd(source, lz);
  const edited = structuredClone(doc.scene);
  edited.elements[1].link = "[[Nueva]]";
  const out = format.writeMd(source, edited, lz, doc.original);
  assert.match(out, /## Element Links\nr1r2r3r4: \[\[Nueva\]\]\n\n%%\n## Drawing/);
  assert.doesNotMatch(out, /Notas/);
});

test("a removed link leaves no Element Links heading behind", () => {
  const source = fixture("ejemplo-json-plano.excalidraw.md");
  const doc = format.readMd(source, lz);
  const edited = structuredClone(doc.scene);
  edited.elements[1].link = null;
  const out = format.writeMd(source, edited, lz, doc.original);
  assert.match(out, /## Text Elements\nHola mundo \^a1b2c3d4\n\n%%\n## Drawing/);
  assert.doesNotMatch(out, /Element Links/);
});

test("a linked shape with a long Excalidraw id gets an 8-character id in Element Links, and its arrows follow", () => {
  const source = fixture("ejemplo-json-plano.excalidraw.md");
  const doc = format.readMd(source, lz);
  const shape = { id: "abcdefghijklmnopqrst", type: "rectangle", link: "[[Nuevo]]", isDeleted: false };
  const arrow = { id: "flecha0001", type: "arrow", startBinding: { elementId: "abcdefghijklmnopqrst" }, isDeleted: false };
  const out = format.writeMd(source, { ...doc.scene, elements: [...doc.scene.elements, shape, arrow] }, lz, doc.original);
  const id = out.match(/^(\S{8}): \[\[Nuevo\]\]$/m)?.[1];
  assert.ok(id, "the link is in Element Links under an 8-character id");
  const back = format.readMd(out, lz).scene.elements;
  assert.equal(back.find((e) => e.link === "[[Nuevo]]").id, id);
  assert.equal(back.find((e) => e.id === "flecha0001").startBinding.elementId, id);
});

test("empty texts stay in the JSON but not in Text Elements", () => {
  const source = fixture("ejemplo-json-plano.excalidraw.md");
  const doc = format.readMd(source, lz);
  const empty = { id: "vacio001", type: "text", text: "", originalText: "", isDeleted: false };
  const spaces = { id: "espacio1", type: "text", text: "  ", originalText: "  ", isDeleted: false };
  const out = format.writeMd(source, { ...doc.scene, elements: [...doc.scene.elements, empty, spaces] }, lz, doc.original);
  const section = out.slice(out.indexOf("## Text Elements"), out.indexOf("## Element Links"));
  assert.equal(section, "## Text Elements\nHola mundo ^a1b2c3d4\n\n");
  assert.ok(format.readMd(out, lz).scene.elements.some((e) => e.id === "vacio001"));
});

test("deleted texts leave the section", () => {
  const source = fixture("ejemplo-json-plano.excalidraw.md");
  const doc = format.readMd(source, lz);
  const deleted = { ...structuredClone(doc.scene.elements[0]), isDeleted: true };
  const out = format.writeMd(source, { ...doc.scene, elements: [deleted] }, lz, doc.original);
  assert.doesNotMatch(out.slice(0, out.indexOf("## Drawing")), /Hola mundo \^/);
});

test("a compressed drawing is written compressed, and reads back the same", () => {
  const source = fixture("ejemplo-json-plano.excalidraw.md");
  const doc = format.readMd(source, lz);
  const el = structuredClone(doc.scene.elements[0]);
  el.text = el.originalText = "x".repeat(600);
  const plain = format.writeMd(source, { ...doc.scene, elements: [el] }, lz, {});
  assert.match(plain, /```json\n/, "plain stays plain");
  const compressedSource = fixture("ejemplo-compressed-json.excalidraw.md");
  const c = format.readMd(compressedSource, lz);
  const out = format.writeMd(compressedSource, { ...c.scene, elements: [el] }, lz, c.original);
  assert.match(out, /```compressed-json\n/);
  assert.match(out, /\n\n/, "chunks are separated by blank lines");
  const back = format.readMd(out, lz);
  assert.equal(back.compressed, true);
  assert.equal(back.scene.elements[0].text, "x".repeat(600));
});

test("a hand-made file without Text Elements gets the section before the drawing", () => {
  const source = "---\nexcalidraw-plugin: parsed\n---\n\n# Dibujo\n\n%%\n## Drawing\n```json\n{}\n```\n%%";
  const doc = format.readMd(source.replace("{}", JSON.stringify({ elements: [] })), lz);
  const withText = { ...doc.scene, elements: [{ id: "abcdefgh", type: "text", text: "Hola", originalText: "Hola", isDeleted: false }] };
  const out = format.writeMd(source.replace("{}", JSON.stringify({ elements: [] })), withText, lz, {});
  assert.match(out, /# Excalidraw Data\n\n## Text Elements\nHola \^abcdefgh\n\n%%\n## Drawing\n/);
});

test("a file without a drawing or with a broken block is an error, never an empty drawing", () => {
  assert.throws(() => format.readMd("# Nota\n\ntexto", lz), /## Drawing/);
  assert.throws(() => format.readMd("## Drawing\n```json\n{\"elements\":", lz), /cierra|JSON/);
  assert.throws(() => format.readMd("## Drawing\n```compressed-json\n@@@\n```\n", lz), /descomprimir/);
  assert.throws(() => format.readMd("## Drawing\n```json\n{\"type\":\"x\"}\n```\n", lz), /elementos/);
});

test("compression round trip, across chunks", () => {
  const json = JSON.stringify({ elements: [{ text: "ñandú ".repeat(200) }] }, null, "\t");
  const packed = format.compressBlock(json, lz);
  assert.ok(packed.split("\n\n").every((c) => c.length <= 256));
  assert.equal(format.decompressBlock(packed, lz), json);
});

test("a .excalidraw file is read as its JSON", () => {
  const scene = format.readJson(JSON.stringify({ type: "excalidraw", elements: [], files: {} }));
  assert.deepEqual(scene.elements, []);
  assert.throws(() => format.readJson("{}"), /elementos/);
});
