// Ctrl+F inside visor's own pages (vault and loose HTML). WebKit's find cannot
// say which match is current, so the page searches its text itself and marks
// the matches with CSS Custom Highlights. Injected by `ui/page_calls.rs` on the
// first search.
(() => {
  "use strict";

  const MAX_MATCHES = 1000;
  const SKIPPED = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEXTAREA"]);
  const WORD_CHAR = /[\p{L}\p{N}]/u;
  const MARKS = /\p{M}/gu;

  // Accents and case are ignored, as WebKit does: "camión" finds "CAMION".
  // Folding a code point may give more than one character, and each of them
  // points back to the same place in the text.
  function foldChar(ch) {
    return ch.normalize("NFD").replace(MARKS, "").toLowerCase();
  }

  // Walks the text nodes under `root` and returns the folded text with, for
  // each folded character, the node and offset where it came from. Text in
  // different blocks, or on both sides of a <br>, is kept apart by a line
  // break: the whitespace between blocks has no boxes and is skipped, so
  // without it "<h2>Resumen</h2><p>El" would read "ResumenEl".
  function foldWithMap(root, matchCase) {
    const walker = root.ownerDocument.createTreeWalker(root, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
      acceptNode(node) {
        if (node.nodeType === Node.ELEMENT_NODE) return node.tagName === "BR" ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
        const parent = node.parentElement;
        if (!parent || SKIPPED.has(parent.tagName) || !visible(parent) || !drawn(node)) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    const parts = [];
    const map = [];
    let block = null;
    let broken = false;
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (node.nodeType === Node.ELEMENT_NODE) {
        broken = true;
        continue;
      }
      const here = blockOf(node.parentElement);
      if (parts.length && (broken || here !== block)) {
        parts.push("\n");
        map.push(map[map.length - 1]);
      }
      block = here;
      broken = false;
      const text = node.data;
      let offset = 0;
      for (const ch of text) {
        const folded = matchCase ? ch : foldChar(ch);
        for (let i = 0; i < folded.length; i++) {
          parts.push(folded[i]);
          map.push({ node, start: offset, end: offset + ch.length });
        }
        offset += ch.length;
      }
    }
    return { text: parts.join(""), map };
  }

  // visor's own boxes (notices, the editor's status) and text nobody can see
  // would count as matches the person cannot reach.
  const seen = new Map();
  function visible(element) {
    if (!seen.has(element)) {
      seen.set(element, !element.closest("[data-visor-skip]") && (!element.checkVisibility || element.checkVisibility()));
    }
    return seen.get(element);
  }

  // The nearest ancestor laid out as a block, table cell, list item…: text
  // in the same one reads as one run.
  const blocks = new Map();
  function blockOf(element) {
    let block = blocks.get(element);
    if (block === undefined) {
      const display = getComputedStyle(element).display;
      const inline = display === "inline" || display === "contents";
      block = inline && element.parentElement ? blockOf(element.parentElement) : element;
      blocks.set(element, block);
    }
    return block;
  }

  // checkVisibility() says yes to text placed straight in a closed <details>;
  // text that is not laid out has no boxes.
  const probe = new Range();
  function drawn(node) {
    probe.selectNodeContents(node);
    return probe.getClientRects().length > 0;
  }

  function isWordChar(ch) {
    return ch !== undefined && WORD_CHAR.test(ch);
  }

  // Positions in the folded text where `needle` starts and ends a whole word.
  // Both sides are checked here; WebKit's own option, used on websites, only
  // checks the start.
  function findAll(haystack, needle, wholeWord) {
    const found = [];
    if (needle === "") return found;
    let at = haystack.indexOf(needle);
    while (at !== -1 && found.length < MAX_MATCHES) {
      const end = at + needle.length;
      if (!wholeWord || (!isWordChar(haystack[at - 1]) && !isWordChar(haystack[end]))) {
        found.push([at, end]);
      }
      at = haystack.indexOf(needle, end);
    }
    return found;
  }

  const state = { ranges: [], current: 0 };

  function rangeFor(folded, [start, end]) {
    const first = folded.map[start];
    const last = folded.map[end - 1];
    const range = new Range();
    range.setStart(first.node, first.start);
    range.setEnd(last.node, last.end);
    return range;
  }

  function paint() {
    if (!("highlights" in CSS)) return;
    const all = new Highlight(...state.ranges);
    CSS.highlights.set("visor-find", all);
    const current = state.ranges[state.current - 1];
    if (current) CSS.highlights.set("visor-find-current", new Highlight(current));
    else CSS.highlights.delete("visor-find-current");
  }

  function show() {
    const range = state.ranges[state.current - 1];
    if (!range) return;
    const node = range.startContainer;
    const element = node.nodeType === Node.ELEMENT_NODE ? node : node.parentElement;
    if (element) element.scrollIntoView({ block: "center", inline: "nearest" });
    paint();
  }

  function result() {
    return { current: state.ranges.length ? state.current : 0, total: state.ranges.length };
  }

  // The highlight colours are set here, not in the page CSS, so the pages that
  // never search carry no extra rules.
  function style() {
    const sheet = document.createElement("style");
    sheet.id = "visor-find-style";
    sheet.textContent =
      "::highlight(visor-find){background-color:#ffe066;color:#000}" +
      "::highlight(visor-find-current){background-color:#ff8c1a;color:#000}";
    document.head.appendChild(sheet);
  }

  const api = {
    // Searches the whole page for `text` and shows the first match.
    search(text, matchCase, wholeWord) {
      seen.clear();
      blocks.clear();
      const folded = foldWithMap(document.body, !!matchCase);
      const needle = matchCase ? text : foldChar(text);
      const hits = findAll(folded.text, needle, !!wholeWord);
      state.ranges = hits.map((hit) => rangeFor(folded, hit));
      // From where the person is reading, not from the top of the page.
      const below = state.ranges.findIndex((range) => range.getBoundingClientRect().bottom >= 0);
      state.current = state.ranges.length ? Math.max(below, 0) + 1 : 0;
      show();
      if (!state.ranges.length) paint();
      return result();
    },
    next() {
      if (state.ranges.length) {
        state.current = state.current >= state.ranges.length ? 1 : state.current + 1;
        show();
      }
      return result();
    },
    previous() {
      if (state.ranges.length) {
        state.current = state.current <= 1 ? state.ranges.length : state.current - 1;
        show();
      }
      return result();
    },
    clear() {
      state.ranges = [];
      state.current = 0;
      if ("highlights" in CSS) {
        CSS.highlights.delete("visor-find");
        CSS.highlights.delete("visor-find-current");
      }
      return result();
    },
  };

  if (!document.getElementById("visor-find-style")) style();
  window.__visorFind = api;
})();
