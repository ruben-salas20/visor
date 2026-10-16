import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { CONTEXT, locateIn, quoteAt, compareTargets } = require("../../../src/page/annot-quote.js");

const text = "La onda sonora viaja. La onda luminosa no. Una onda corta.";

test("quoteAt keeps the exact text, its context and the start", () => {
  const start = text.indexOf("onda luminosa");
  const q = quoteAt(text, start, start + "onda luminosa".length);
  assert.equal(q.exact, "onda luminosa");
  assert.equal(q.start, start);
  assert.equal(q.prefix, text.slice(Math.max(0, start - CONTEXT), start));
  assert.equal(q.suffix, text.slice(start + 13, start + 13 + CONTEXT));
});

test("locateIn finds the quote again where its context matches", () => {
  const start = text.indexOf("onda luminosa");
  const q = quoteAt(text, start, start + 13);
  // The text moved: the same words now sit elsewhere, with the same context.
  const moved = "Intro nueva. " + text;
  const hit = locateIn(moved, q);
  assert.deepEqual(hit, { start: start + 13, end: start + 13 + 13 });
  assert.equal(moved.slice(hit.start, hit.end), "onda luminosa");
});

test("among equal occurrences, the one nearest the old start wins", () => {
  const twice = "onda. onda. onda.";
  // Occurrences at 0, 6 and 12: from 10, the one at 12 is nearest.
  const hit = locateIn(twice, { exact: "onda", start: 10 });
  assert.equal(hit.start, 12);
});

test("context beats distance when it tells the occurrences apart", () => {
  const body = "el banco del río. el banco de la plaza.";
  const second = body.indexOf("de la plaza") - 6;
  const q = { exact: "banco", prefix: "el ", suffix: " de la", start: 0 };
  const hit = locateIn(body, q);
  assert.equal(hit.start, second);
});

test("a missing or empty quote is not found", () => {
  assert.equal(locateIn(text, { exact: "" }), null);
  assert.equal(locateIn(text, { exact: "ausente" }), null);
  assert.equal(locateIn(text, {}), null);
});

test("a context cut inside an emoji leaves no lone surrogate", () => {
  // The emoji is at 31-32; a start at 64 makes the 32-unit prefix begin on its low half.
  const s = "a".repeat(31) + "😀" + "b".repeat(40);
  const q = quoteAt(s, 64, 65);
  assert.doesNotMatch(q.prefix, /[\uD800-\uDFFF]/);
  assert.equal(q.exact, "b");
  const emoji = quoteAt("😀", 0, 1);
  assert.equal(emoji.exact, "�", "a lone half of a pair is replaced, never kept");
});

test("PDF notes sort by page, then by offset; HTML notes by offset", () => {
  const notes = [
    { page: 2, quote: { start: 5 } },
    { page: 1, quote: { start: 90 } },
    { page: 1, quote: { start: 10 } },
  ];
  notes.sort((a, b) => compareTargets(a, b));
  assert.deepEqual(notes.map((n) => [n.page, n.quote.start]), [[1, 10], [1, 90], [2, 5]]);
  const html = [{ quote: { start: 40 } }, { quote: { start: 3 } }];
  html.sort(compareTargets);
  assert.deepEqual(html.map((n) => n.quote.start), [3, 40]);
});
