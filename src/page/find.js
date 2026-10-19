// Ctrl+F inside visor's own pages (vault and loose HTML, and stand-alone .svg
// documents, whose root is `document.documentElement`). WebKit's find cannot
// say which match is current, so the page searches its text itself and marks
// the matches with CSS Custom Highlights. Injected by `ui/page_calls.rs` on
// the first search. `foldChar`, `foldRuns`, `locate`, `findAll`, `centered`
// and `nearest` take no DOM: tests in tools/find/test.
(() => {
  "use strict";

  const MAX_MATCHES = 1000;
  const SKIPPED = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEXTAREA"]);
  const WORD_CHAR = /[\p{L}\p{N}]/u;
  const MARKS = /\p{M}/gu;
  const SVG_NS = "http://www.w3.org/2000/svg";
  // Values of `overflow` that let the person scroll the box.
  const SCROLLS = new Set(["auto", "scroll", "overlay"]);

  // Accents and case are ignored, as WebKit does: "camión" finds "CAMION".
  // Folding one code point may give several characters, and all of them come
  // from the same place of the text.
  function foldChar(ch) {
    return ch.normalize("NFD").replace(MARKS, "").toLowerCase();
  }

  // Folds the text of `runs`, in the order the page walked it, into the one
  // string `findAll` looks in. Every run brings `sep` (a <br>, or a line of
  // the layout, went between it and the run before) and `block` (whatever
  // stands for its block: text on the same one reads as one run). Instead of
  // one object per character, each run carries two integers per folded
  // character (`starts` and `lens`, next to the node that owns the run), so a
  // match far into the page does not cost a heap of small objects.
  function foldRuns(runs, matchCase) {
    const parts = [];
    const nodes = [];
    let block = null;
    for (const run of runs) {
      if (parts.length && (run.sep || run.block !== block)) parts.push("\n");
      block = run.block;
      const entry = { node: run.node, at: parts.length, starts: [], lens: [] };
      let offset = 0;
      for (const ch of run.text) {
        const folded = matchCase ? ch : foldChar(ch);
        for (let i = 0; i < folded.length; i++) {
          parts.push(folded[i]);
          entry.starts.push(offset);
          entry.lens.push(ch.length);
        }
        offset += ch.length;
      }
      if (entry.starts.length) nodes.push(entry);
    }
    return { text: parts.join(""), nodes };
  }

  // The node, the offset and the length in it of folded position `at`; null on
  // a newline between blocks, where a match can neither start nor end.
  function locate(nodes, at) {
    let low = 0;
    for (let high = nodes.length - 1; low < high; ) {
      const middle = (low + high + 1) >> 1;
      if (nodes[middle].at <= at) low = middle; else high = middle - 1;
    }
    const entry = nodes[low];
    if (!entry) return null;
    const local = at - entry.at;
    if (local < 0 || local >= entry.starts.length) return null;
    return { node: entry.node, start: entry.starts[local], end: entry.starts[local] + entry.lens[local] };
  }

  function isWordChar(ch) {
    return ch !== undefined && WORD_CHAR.test(ch);
  }

  // Positions in the folded text where `needle` starts and ends a whole word.
  // Both sides are checked here; WebKit's own option, used on websites, only
  // checks the start. One past the limit is looked for, to know there are more.
  function findAll(haystack, needle, wholeWord) {
    const found = [];
    if (needle === "") return found;
    let at = haystack.indexOf(needle);
    while (at !== -1 && found.length <= MAX_MATCHES) {
      const end = at + needle.length;
      if (!wholeWord || (!isWordChar(haystack[at - 1]) && !isWordChar(haystack[end]))) {
        found.push([at, end]);
      }
      at = haystack.indexOf(needle, end);
    }
    return found;
  }

  // How far to scroll a box that sees `size` pixels from `from` on, so that the
  // stretch from `start` to `end` stands in its middle. A stretch longer than
  // the box is placed with its start at the start.
  function centered(start, end, from, size) {
    const length = end - start;
    return start - from - (length < size ? (size - length) / 2 : 0);
  }

  // The same, but scrolling only as little as brings the stretch into sight.
  function nearest(start, end, from, size) {
    if (start < from || end - start > size) return start - from;
    if (end > from + size) return end - from - size;
    return 0;
  }

  function rangeFor(folded, [start, end]) {
    const first = locate(folded.nodes, start);
    const last = locate(folded.nodes, end - 1);
    if (!first || !last) return null;
    const range = new Range();
    range.setStart(first.node, first.start);
    range.setEnd(last.node, last.end);
    return range;
  }

  const state = { ranges: [], current: 0, more: false };

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
    // Scrolled by the box of the match itself, not by the one of the element
    // that holds it: with the element, a match at the end of a <pre> taller
    // than the window could stay out of sight while the counter said «5 de 12».
    // The boxes that scroll on their own (a <main> with `overflow: auto`, a
    // wide <pre>) go first, from the inside out; the window, last.
    const start = range.startContainer;
    let element = start.nodeType === Node.ELEMENT_NODE ? start : start.parentElement;
    for (; element && element !== document.scrollingElement; element = element.parentElement) {
      const box = range.getBoundingClientRect();
      if (!box.width && !box.height) break;
      const style = getComputedStyle(element);
      const port = element.getBoundingClientRect();
      const top = port.top + element.clientTop;
      const left = port.left + element.clientLeft;
      const down = SCROLLS.has(style.overflowY) && element.scrollHeight > element.clientHeight;
      const across = SCROLLS.has(style.overflowX) && element.scrollWidth > element.clientWidth;
      if (down || across) {
        // "instant": a page with `scroll-behavior: smooth` would still be on
        // its way when the next box out measures where the match is.
        element.scrollBy({
          top: down ? centered(box.top, box.bottom, top, element.clientHeight) : 0,
          left: across ? nearest(box.left, box.right, left, element.clientWidth) : 0,
          behavior: "instant",
        });
      }
    }
    const box = range.getBoundingClientRect();
    if (box.width || box.height) {
      window.scrollTo({
        top: window.scrollY + centered(box.top, box.bottom, 0, window.innerHeight),
        left: window.scrollX + nearest(box.left, box.right, 0, window.innerWidth),
        behavior: "instant",
      });
    }
    paint();
  }

  function result() {
    return { current: state.ranges.length ? state.current : 0, total: state.ranges.length, more: state.more };
  }

  // -- What only a page can do -------------------------------------------------

  // The container of the text to walk: an .svg has its root there.
  function root() {
    return document.body || document.documentElement;
  }

  // Walks the text nodes under `root` and makes one run of each one, with the
  // block and the <br>s around it already resolved.
  function foldAll(root, matchCase) {
    // The answers are kept per walk: every `checkVisibility()` and every
    // `getClientRects()`, if asked for one key and then the next, gets the
    // same layout once WebKit has gone through it, so the only cost is the
    // first one.
    const seen = new WeakMap();
    const blocks = new WeakMap();
    const laid = new WeakMap();

    // visor's own boxes (notices, the editor's status) and text nobody can see
    // would count as matches the person cannot reach.
    const visible = (element) => {
      let yes = seen.get(element);
      if (yes === undefined) {
        yes = !element.closest("[data-visor-skip]") && (!element.checkVisibility || element.checkVisibility());
        seen.set(element, yes);
      }
      return yes;
    };

    // The nearest ancestor laid out as a block, table cell, list item…: text
    // in the same one reads as one run.
    const blockOf = (element) => {
      let block = blocks.get(element);
      if (block === undefined) {
        const display = getComputedStyle(element).display;
        const inline = display === "inline" || display === "contents";
        block = inline && element.parentElement ? blockOf(element.parentElement) : element;
        blocks.set(element, block);
      }
      return block;
    };

    // checkVisibility() says yes to text placed straight in a closed <details>;
    // text that is not laid out has no boxes.
    const probe = new Range();
    const drawn = (node) => {
      let yes = laid.get(node);
      if (yes === undefined) {
        probe.selectNodeContents(node);
        yes = probe.getClientRects().length > 0;
        laid.set(node, yes);
      }
      return yes;
    };

    const walker = root.ownerDocument.createTreeWalker(root, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT, {
      acceptNode(node) {
        if (node.nodeType === Node.ELEMENT_NODE) return node.tagName === "BR" ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
        const parent = node.parentElement;
        if (!parent || SKIPPED.has(parent.tagName) || !visible(parent) || !drawn(node)) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    const runs = [];
    let broken = false;
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (node.nodeType === Node.ELEMENT_NODE) {
        broken = true;
        continue;
      }
      runs.push({ sep: broken, text: node.data, node, block: blockOf(node.parentElement) });
      broken = false;
    }
    return foldRuns(runs, matchCase);
  }

  // The folded text is worth what one walk costs, not one per key: it is kept
  // until the page changes, and any change of tree or attributes (a class or
  // a style can also hide text) throws it away. Between keys, `search` reads
  // the tree only once again. Only a page has changes; the watcher starts
  // with the search, below.
  let cached = null;

  function pageText(matchCase) {
    if (!cached || cached.matchCase !== matchCase) {
      cached = foldAll(root(), matchCase);
      cached.matchCase = matchCase;
    }
    return cached;
  }

  // The highlight colours are set here, not in the page CSS, so the pages that
  // never search carry no extra rules. An .svg has no <head>: its `<style>`
  // goes into the root svg.
  function style() {
    const sheet = root().namespaceURI === SVG_NS
      ? document.createElementNS(SVG_NS, "style")
      : document.createElement("style");
    sheet.id = "visor-find-style";
    sheet.textContent =
      "::highlight(visor-find){background-color:#ffe066;color:#000}" +
      "::highlight(visor-find-current){background-color:#ff8c1a;color:#000}";
    (document.head || root()).appendChild(sheet);
  }

  const api = {
    // Searches the whole page for `text` and shows the first match. Between
    // keys only the text typed changed, so the folded text of the page does
    // not change with them.
    search(text, matchCase, wholeWord) {
      const folded = pageText(!!matchCase);
      const needle = matchCase ? text : foldChar(text);
      const hits = findAll(folded.text, needle, !!wholeWord);
      state.more = hits.length > MAX_MATCHES;
      hits.length = Math.min(hits.length, MAX_MATCHES);
      state.ranges = hits.map((hit) => rangeFor(folded, hit)).filter(Boolean);
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
      state.more = false;
      if ("highlights" in CSS) {
        CSS.highlights.delete("visor-find");
        CSS.highlights.delete("visor-find-current");
      }
      return result();
    },
  };

  // Asked by `document`, not by `module`: an element with id="module" makes a
  // global `module` in the page.
  if (typeof document === "undefined") {
    // Node tests: only the DOM-free engine.
    module.exports = { foldChar, foldRuns, locate, findAll, centered, nearest };
  } else {
    // One walk per change of the page: any change of tree or attributes
    // throws the folded text away.
    new MutationObserver(() => { cached = null; }).observe(document, {
      childList: true,
      subtree: true,
      characterData: true,
      attributes: true,
    });
    if (!document.getElementById("visor-find-style")) style();
    window.__visorFind = api;
  }
})();
