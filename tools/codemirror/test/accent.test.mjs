// node --test tools/codemirror/test/accent.test.mjs
// The search without «mayúsculas» (src/page/editor.js): accents and case are ignored.
import { test } from "node:test";
import assert from "node:assert/strict";
import { accentPattern } from "../accent.mjs";

const find = (query, text) => new RegExp(accentPattern(query), "giu").test(text);

test("arbol finds árbol and ÁRBOL, and arbol", () => {
  assert.ok(find("arbol", "un árbol"));
  assert.ok(find("arbol", "ÁRBOL"));
  assert.ok(find("arbol", "arbol"));
});

test("an accented query finds the plain text too", () => {
  assert.ok(find("árbol", "arbol"));
  assert.ok(find("niño", "nino"));
  assert.ok(find("ÑANDÚ", "nandu"));
});

test("punctuation is escaped", () => {
  assert.ok(find("a.b", "a.b"));
  assert.ok(!find("a.b", "axb"));
  assert.ok(!find("(x)", "x"));
});

test("decomposed input is composed first", () => {
  assert.equal(accentPattern("ü"), accentPattern("ü"));
});
