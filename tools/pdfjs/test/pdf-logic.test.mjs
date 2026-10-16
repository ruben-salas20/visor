// Run with «node --test tools/pdfjs/test/». The logic is the UMD module the viewer page also loads.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";

const { parsePage, pageLabel, matchLabel, hasOutline } = createRequire(import.meta.url)("../../../src/page/pdf-logic.js");

test("a typed page is kept only when it is a page of the document", () => {
  assert.equal(parsePage("3", 5), 3);
  assert.equal(parsePage(" 2 ", 5), 2);
  assert.equal(parsePage("1", 5), 1);
  assert.equal(parsePage("5", 5), 5);
  assert.equal(parsePage("0", 5), null);
  assert.equal(parsePage("6", 5), null);
  assert.equal(parsePage("", 5), null);
  assert.equal(parsePage("2.0", 5), null);
  assert.equal(parsePage("-1", 5), null);
  assert.equal(parsePage("1e1", 50), null);
  assert.equal(parsePage("abc", 5), null);
});

test("the page label fills both marks", () => {
  assert.equal(pageLabel("Página {n} de {m}", 4, 12), "Página 4 de 12");
  assert.equal(pageLabel("Page {n} of {m}", 1, 1), "Page 1 of 1");
});

test("the match label shows the position, or the no-match text", () => {
  assert.equal(matchLabel("{n} de {m}", "Sin resultados", 3, 12), "3 de 12");
  assert.equal(matchLabel("{n} de {m}", "Sin resultados", 0, 0), "Sin resultados");
});

test("the outline appears only when the document has entries", () => {
  assert.equal(hasOutline(null), false);
  assert.equal(hasOutline(undefined), false);
  assert.equal(hasOutline([]), false);
  assert.equal(hasOutline([{ title: "Capítulo 1", dest: "p1" }]), true);
});
