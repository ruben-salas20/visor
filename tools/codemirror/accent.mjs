// Search text that ignores accents: each letter becomes the class of its
// accented forms. Case is left to the regexp flag "i", which folds «Á» to «á».
// The classes take every letter that loses its marks to a single letter, as the
// reading view's search does (src/page/find.js): Latin, Greek and Cyrillic.
const ranges = [[0xc0, 0x24f], [0x370, 0x52f], [0x1e00, 0x1fff]];
const LETTER = /^\p{L}$/u;

function strip(ch) {
  return ch.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

const forms = new Map();
for (const [from, to] of ranges) {
  for (let code = from; code <= to; code++) {
    const ch = String.fromCodePoint(code).toLowerCase();
    const base = strip(ch);
    if (base === ch || !LETTER.test(base) || ch.length !== 1) continue;
    if (!forms.has(base)) forms.set(base, new Set([base]));
    forms.get(base).add(ch);
  }
}
// Text that keeps a letter and its mark apart (pasted from some PDFs) still
// matches: the marks after a letter are skipped.
const classes = new Map([...forms].map(([base, set]) => [base, `[${[...set].join("")}]\\p{M}*`]));

// NFC first, so a letter and its combining mark (typed on some keyboards) count
// as one. «\n», «\r», «\t» and «\\» name the character, as in CodeMirror's own
// search without regexp: a selection over several lines arrives written so.
export function accentPattern(text) {
  const plain = text.replace(/\\([nrt\\])/g, (_, ch) => ({ n: "\n", r: "\r", t: "\t" })[ch] ?? "\\");
  let out = "";
  for (const ch of plain.normalize("NFC")) {
    out += classes.get(strip(ch)) ?? ch.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }
  return out;
}
