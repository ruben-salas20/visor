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

test("letters beyond Spanish lose their marks too, as in the reading view", () => {
  assert.ok(find("dvorak", "Dvořák"));
  assert.ok(find("sao paulo", "São Paulo"));
  assert.ok(!find("lodz", "Łódź")); // «Ł» has no mark to lose, in either search
  assert.ok(find("odz", "Łódź"));
  assert.ok(find("αθηνα", "Αθήνα"));
});

test("text that keeps the letter and its mark apart is found", () => {
  assert.ok(find("arbol", "árbol"));
  assert.ok(find("árbol", "árbol"));
});

test("\\n, \\t and \\\\ name the character, as CodeMirror's plain search does", () => {
  assert.ok(find("uno\\ndos", "uno\ndos"));
  assert.ok(!find("uno\\ndos", "uno\\ndos"));
  assert.ok(find("a\\tb", "a\tb"));
  assert.ok(find("a\\\\b", "a\\b"));
});
