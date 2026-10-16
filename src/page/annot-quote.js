// Text anchors for the annotation layer, without DOM: a quote is found again by
// its exact text and its context, and its start is a character offset in the
// page's text (the whole page for HTML and Markdown, one PDF page for PDFs).
// Loaded before annot.js, which keeps the DOM part.
(() => {
  const CONTEXT = 32;

  // A slice of the text may start or end inside a surrogate pair: drop the stray half.
  const whole = (s) => s.replace(/^[\uDC00-\uDFFF]/, "").replace(/[\uD800-\uDBFF]$/, "").toWellFormed();

  // The occurrence whose surroundings match best, and among equals the one closest
  // to where the quote used to start. Returns { start, end } or null.
  function locateIn(text, quote) {
    const { exact, prefix = "", suffix = "", start = 0 } = quote;
    if (!exact) return null;
    let best = -1, bestScore = -Infinity;
    for (let i = text.indexOf(exact); i !== -1; i = text.indexOf(exact, i + 1)) {
      const before = text.slice(Math.max(0, i - prefix.length), i);
      const after = text.slice(i + exact.length, i + exact.length + suffix.length);
      let score = 0;
      while (score < before.length && before[before.length - 1 - score] === prefix[prefix.length - 1 - score]) score++;
      let s = 0;
      while (s < after.length && after[s] === suffix[s]) s++;
      score += s - Math.abs(i - start) / 1e6;
      if (score > bestScore) { best = i; bestScore = score; }
    }
    return best === -1 ? null : { start: best, end: best + exact.length };
  }

  function quoteAt(text, start, end) {
    return {
      exact: text.slice(start, end).toWellFormed(),
      prefix: whole(text.slice(Math.max(0, start - CONTEXT), start)),
      suffix: whole(text.slice(end, end + CONTEXT)),
      start,
    };
  }

  // Notes in reading order: by PDF page, then by offset inside the page.
  // HTML and Markdown notes have no page and keep their offset order.
  function compareTargets(a, b) {
    return (a.page ?? 0) - (b.page ?? 0) || (a.quote?.start ?? 1e12) - (b.quote?.start ?? 1e12);
  }

  const api = { CONTEXT, locateIn, quoteAt, compareTargets };
  if (typeof module !== "undefined") module.exports = api;
  else window.__visorAnnotQuote = api;
})();
