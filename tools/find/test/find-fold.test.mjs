// node --test tools/find/test/find-fold.test.mjs
// Text engine of the find bar (src/page/find.js): folding, finding and
// locating, without DOM. The runs here stand where the page's walker puts its
// text nodes; `block` is any value, compared by identity.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const { foldChar, foldRuns, locate, findAll, centered, nearest } = createRequire(import.meta.url)(
  join(here, "..", "..", "..", "src", "page", "find.js")
);

// One run per text node; `node` is whatever can be told apart later.
const run = (text, node = { name: text }, block = node, sep = false) => ({ sep, text, node, block });

test("accents and case fold as WebKit does", () => {
  assert.equal(foldChar("Ñ"), "n");
  assert.equal(foldChar("é"), "e");
  assert.equal(foldChar("A"), "a");
  // But not everything: the sharp s and the ligatures have no decomposition.
  assert.equal(foldChar("ß"), "ß");
  assert.equal(foldChar("ﬁ"), "ﬁ");
});

test("a code point can fold to more than one character", () => {
  // The syllable «각» decomposes to its three jamo, all from the same place.
  assert.equal(foldChar("각").length, 3);
});

test("runs of the same block read as one run", () => {
  const block = {};
  const folded = foldRuns([run("Resumen", "a", block), run("El ", "b", block)], true);
  assert.equal(folded.text, "ResumenEl ");
  assert.deepEqual(
    folded.nodes.map((n) => n.at),
    [0, 7],
  );
});

test("two blocks, and a break after a <br>, are separated by a newline", () => {
  const folded = foldRuns([run("Resumen", "a", {}), run("El", "b", {}), run("mar", "c", {}, true)], true);
  assert.equal(folded.text, "Resumen\nEl\nmar");
});

test("each folded character knows the node, the offset and the length it came from", () => {
  const node = { name: "camión" };
  const folded = foldRuns([run("Camión", node)], false);
  assert.equal(folded.text, "camion");
  // «ó» folds to one character that comes from offset 4, length 1.
  assert.deepEqual(locate(folded.nodes, 4), { node, start: 4, end: 5 });
  assert.deepEqual(locate(folded.nodes, 0), { node, start: 0, end: 1 });
  assert.deepEqual(locate(folded.nodes, 5), { node, start: 5, end: 6 });
});

test("every character of a multi-character fold comes from the same place", () => {
  const node = { name: "각a" };
  const folded = foldRuns([run("각a", node)], false);
  // Three folded characters, and each of the three points at offset 0.
  assert.equal(folded.text.length, 4);
  assert.deepEqual(locate(folded.nodes, 0), { node, start: 0, end: 1 });
  assert.deepEqual(locate(folded.nodes, 1), { node, start: 0, end: 1 });
  assert.deepEqual(locate(folded.nodes, 2), { node, start: 0, end: 1 });
  assert.deepEqual(locate(folded.nodes, 3), { node, start: 1, end: 2 });
});

test("past the last character there is nothing to point at", () => {
  const folded = foldRuns([run("mar", "a", {}), run("y", "b", {})], false);
  // A match can neither start nor end in the newline between both.
  assert.equal(folded.text, "mar\ny");
  assert.equal(locate(folded.nodes, 3), null);
  assert.deepEqual(locate(folded.nodes, 4), { node: "b", start: 0, end: 1 });
  assert.deepEqual(locate(folded.nodes, 0), { node: "a", start: 0, end: 1 });
  assert.equal(locate([], 0), null);
  assert.equal(locate([], 99), null);
});

test("a position far into the text is found with a binary search", () => {
  const folded = foldRuns(
    ["uno", "dos", "tres"].map((text) => run(text, { name: text }, {})),
    false,
  );
  assert.equal(folded.text, "uno\ndos\ntres");
  // The third block starts at 8; position 10 is its «e».
  assert.deepEqual(locate(folded.nodes, 10), { node: { name: "tres" }, start: 2, end: 3 });
  assert.deepEqual(locate(folded.nodes, 4), { node: { name: "dos" }, start: 0, end: 1 });
  assert.equal(locate(folded.nodes, 3), null);
  assert.equal(locate(folded.nodes, 7), null);
});

test("the search ignores accents and case", () => {
  const { text } = foldRuns([run("CAMIÓN detrás")], false);
  assert.equal(text, "camion detras");
  assert.deepEqual(findAll(text, foldChar("DETRÁS")), [[7, 13]]);
});

test("a whole word does not end inside another", () => {
  const { text } = foldRuns([run("el mar y elsmar el")], false);
  // «el» at 0 and 16, not the one of «elsmar».
  assert.deepEqual(
    findAll(text, "el", true).map(([at]) => at),
    [0, 16],
  );
});

test("past the limit one more match is counted, not every one", () => {
  // The loop looks for one past the limit, to know there are more.
  assert.equal(findAll("aa".repeat(1001), "aa", false).length, 1001);
});

test("a match is centered in the box that scrolls", () => {
  // A box of 1000 px from y=100; a 20 px match at y=2000 ends up at 590.
  assert.equal(centered(2000, 2020, 100, 1000), 1410);
  // Already centered: nothing to scroll.
  assert.equal(centered(590, 610, 100, 1000), 0);
  // Taller than the box: its top edge goes to the top.
  assert.equal(centered(2000, 3500, 100, 1000), 1900);
});

test("sideways, only as much as brings the match into sight", () => {
  assert.equal(nearest(50, 80, 0, 500), 0);
  assert.equal(nearest(600, 650, 0, 500), 150);
  assert.equal(nearest(-40, -10, 0, 500), -40);
  // Wider than the box: its start goes to the start.
  assert.equal(nearest(100, 800, 0, 500), 100);
});
