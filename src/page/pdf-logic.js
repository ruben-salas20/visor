// The parts of the PDF viewer that take no DOM, so node can test them
// (tools/pdfjs/test). The page reads them from window.__visorPdfLogic.
(function (root) {
  "use strict";

  // The page number the user typed, when it is a page of the document. Anything
  // else (text, a decimal, out of range) is not a page, and the caller keeps the current one.
  function parsePage(text, total) {
    const s = String(text).trim();
    if (!/^\d+$/.test(s)) return null;
    const n = Number(s);
    return n >= 1 && n <= total ? n : null;
  }

  // Fills the `{n}` and `{m}` marks of a sentence in the reader's language.
  function fill(template, values) {
    return template.replace(/\{([nm])\}/g, (mark, key) => String(values[key]));
  }

  function pageLabel(template, n, m) {
    return fill(template, { n, m });
  }

  // Without a match the label is the text for that case, not «0 de 0».
  function matchLabel(template, noMatches, current, total) {
    return total === 0 ? noMatches : fill(template, { n: current, m: total });
  }

  // The outline button and panel appear only for a document that has one.
  function hasOutline(outline) {
    return Array.isArray(outline) && outline.length > 0;
  }

  const api = { parsePage, pageLabel, matchLabel, hasOutline };
  if (typeof module !== "undefined") module.exports = api;
  else root.__visorPdfLogic = api;
})(typeof globalThis !== "undefined" ? globalThis : this);
