// Search text that ignores accents: each letter becomes the class of its
// accented forms. Case is left to the regexp flag "i", which folds «Á» to «á».
const groups = ["aáàâäãå", "eéèêë", "iíìîï", "oóòôöõ", "uúùûü", "nñ", "cç", "yýÿ"];
const classes = new Map(groups.map((g) => [g[0], `[${g}]`]));

// NFC first, so a letter and its combining mark (typed on some keyboards) count as one.
export function accentPattern(text) {
  let out = "";
  for (const ch of text.normalize("NFC")) {
    const base = ch.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
    out += classes.get(base) ?? ch.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }
  return out;
}
