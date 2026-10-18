// Note editor with live preview, on CodeMirror 6 (window.CM, from
// assets/codemirror). note.js loads it on the first Ctrl+E, never for reading.
(() => {
  const {
    EditorState, StateField, StateEffect, Prec,
    EditorView, ViewPlugin, Decoration, WidgetType, keymap, drawSelection,
    history, defaultKeymap, historyKeymap, insertTab,
    Language, defineLanguageFacet, languageDataProp, syntaxTree, syntaxHighlighting, HighlightStyle,
    parser, GFM, tags: t,
  } = window.CM;
  const config = window.__visorNoteConfig;
  // Helper named tx: "tr" is already a local name in this file.
  const tx = (es, en) => (config.lang === "en" ? en : es);

  // ---------- Markdown: GFM plus what pulldown-cmark reads in visor ----------

  const space = (code) => code === 32 || code === 9 || code === 10 || Number.isNaN(code);

  const visorSyntax = {
    defineNodes: [
      { name: "InlineMath", style: t.monospace },
      { name: "BlockMath", block: true, style: t.monospace },
      { name: "MathMark", style: t.processingInstruction },
      { name: "WikiLink", style: t.link },
      { name: "WikiMark", style: t.processingInstruction },
      { name: "WikiTarget" },
      { name: "WikiEmbed", style: t.link },
      { name: "FrontMatter", block: true, style: t.meta },
    ],
    parseInline: [
      {
        // $…$ and $$…$$ with pulldown-cmark's rules: no space just inside a single $.
        name: "InlineMath",
        before: "Emphasis",
        parse(cx, next, pos) {
          if (next !== 36) return -1;
          const open = cx.char(pos + 1) === 36 ? 2 : 1;
          const start = pos + open;
          if (open === 1 && (start >= cx.end || space(cx.char(start)) || cx.char(start) === 36)) return -1;
          for (let i = start; i < cx.end; i++) {
            const c = cx.char(i);
            if (c === 92) { i++; continue; }
            if (c !== 36) continue;
            if (open === 2) {
              if (cx.char(i + 1) !== 36) continue;
              if (i === start) return -1;
            } else if (space(cx.char(i - 1))) continue;
            const end = i + open;
            return cx.addElement(cx.elt("InlineMath", pos, end, [cx.elt("MathMark", pos, start), cx.elt("MathMark", i, end)]));
          }
          return -1;
        },
      },
      {
        // ![[image.png]], as in Obsidian: the image is found by name in the vault.
        name: "WikiEmbed",
        before: "Image",
        parse(cx, next, pos) {
          if (next !== 33 || cx.char(pos + 1) !== 91 || cx.char(pos + 2) !== 91) return -1;
          const inner = cx.slice(pos + 3, cx.end);
          const close = inner.indexOf("]]");
          if (close < 1 || /[[\]\n]/.test(inner.slice(0, close))) return -1;
          const end = pos + 3 + close + 2;
          const bar = inner.slice(0, close).indexOf("|");
          const targetEnd = pos + 3 + (bar >= 0 ? bar : close);
          return cx.addElement(cx.elt("WikiEmbed", pos, end, [
            cx.elt("WikiMark", pos, pos + 3), cx.elt("WikiTarget", pos + 3, targetEnd), cx.elt("WikiMark", targetEnd, end),
          ]));
        },
      },
      {
        name: "WikiLink",
        before: "Link",
        parse(cx, next, pos) {
          if (next !== 91 || cx.char(pos + 1) !== 91) return -1;
          const inner = cx.slice(pos + 2, cx.end);
          const close = inner.indexOf("]]");
          if (close < 1 || /[[\]\n]/.test(inner.slice(0, close))) return -1;
          const end = pos + 2 + close + 2;
          const bar = inner.slice(0, close).indexOf("|");
          const parts = [cx.elt("WikiMark", pos, pos + 2)];
          if (bar >= 0) parts.push(cx.elt("WikiTarget", pos + 2, pos + 2 + bar), cx.elt("WikiMark", pos + 2 + bar, pos + 3 + bar));
          else parts.push(cx.elt("WikiTarget", pos + 2, end - 2));
          parts.push(cx.elt("WikiMark", end - 2, end));
          return cx.addElement(cx.elt("WikiLink", pos, end, parts));
        },
      },
    ],
    parseBlock: [
      {
        name: "FrontMatter",
        before: "HorizontalRule",
        parse(cx, line) {
          if (cx.lineStart !== 0 || line.text.trimEnd() !== "---") return false;
          const from = 0;
          while (cx.nextLine()) {
            if (/^(---|\.\.\.)\s*$/.test(line.text)) {
              cx.nextLine();
              break;
            }
          }
          cx.addElement(cx.elt("FrontMatter", from, cx.prevLineEnd()));
          return true;
        },
      },
      {
        name: "BlockMath",
        before: "FencedCode",
        parse(cx, line) {
          if (line.next !== 36 || line.text.charCodeAt(line.pos + 1) !== 36) return false;
          const from = cx.lineStart + line.pos;
          const marks = [cx.elt("MathMark", from, from + 2)];
          const first = line.text.slice(line.pos + 2).trimEnd();
          let closed = first.length > 2 && first.endsWith("$$");
          if (closed) {
            const end = cx.lineStart + line.pos + 2 + first.length;
            marks.push(cx.elt("MathMark", end - 2, end));
          } else {
            while (cx.nextLine() && line.depth >= cx.depth) {
              const text = line.text.trimEnd();
              if (text.endsWith("$$")) {
                marks.push(cx.elt("MathMark", cx.lineStart + text.length - 2, cx.lineStart + text.length));
                closed = true;
                break;
              }
            }
          }
          if (closed) cx.nextLine();
          cx.addElement(cx.elt("BlockMath", from, cx.prevLineEnd(), marks));
          return true;
        },
        endLeaf: (cx, line) => line.next === 36 && line.text.charCodeAt(line.pos + 1) === 36,
      },
    ],
  };

  const facet = defineLanguageFacet();
  const markdown = new Language(
    facet,
    parser.configure([GFM, visorSyntax, { props: [languageDataProp.add({ Document: facet })] }]),
    [],
    "markdown",
  );

  const highlight = HighlightStyle.define([
    { tag: t.heading, class: "md-heading" },
    { tag: t.strong, class: "md-strong" },
    { tag: t.emphasis, class: "md-em" },
    { tag: t.strikethrough, class: "md-strike" },
    { tag: t.monospace, class: "md-code" },
    { tag: t.link, class: "md-link" },
    { tag: t.url, class: "md-url" },
    { tag: [t.processingInstruction, t.labelName, t.string, t.contentSeparator], class: "md-mark" },
    { tag: t.quote, class: "md-quote" },
    { tag: t.meta, class: "md-meta" },
    { tag: t.comment, class: "md-comment" },
  ]);

  // ---------- libraries from assets/, loaded only if the note needs them ----------

  const refresh = StateEffect.define();
  let current = null; // the open EditorView
  const poke = () => current && current.dispatch({ effects: refresh.of(null) });

  const LIBS = {
    katex: { ready: () => window.katex, files: ["katex/katex.min.css", "katex/katex.min.js"] },
    hljs: { ready: () => window.hljs, files: ["hljs/highlight.min.js"] },
    pele: { ready: () => window.pele, files: ["pele/pele.global.js"] },
  };
  const asked = new Set();
  // Returns the library if it is already there; otherwise starts loading it
  // and redraws the editor once it arrives.
  function lib(name) {
    const spec = LIBS[name];
    const ready = spec.ready();
    if (ready || asked.has(name)) return ready;
    asked.add(name);
    for (const file of spec.files) {
      const url = config.assets + file;
      const el = file.endsWith(".css")
        ? Object.assign(document.createElement("link"), { rel: "stylesheet", href: url })
        : Object.assign(document.createElement("script"), { src: url, onload: poke });
      el.onerror = () => console.error(tx("No se pudo cargar ", "Could not load ") + file);
      document.head.append(el);
    }
    return null;
  }

  // ---------- links ----------

  // [[target]] → URL, or null if no page matches; asked to http.rs once per target.
  const wikiUrls = new Map();
  let wikiQueue = new Set();
  let wikiTimer = 0;
  function wikiUrl(target) {
    if (wikiUrls.has(target)) return wikiUrls.get(target);
    wikiQueue.add(target);
    clearTimeout(wikiTimer);
    // Typing inside a link changes its target on each key; ask once it settles.
    wikiTimer = setTimeout(async () => {
      const batch = [...wikiQueue];
      wikiQueue = new Set();
      await Promise.all(batch.map(async (name) => {
        try {
          const response = await fetch(location.pathname + "?wiki=" + encodeURIComponent(name), { cache: "no-store" });
          wikiUrls.set(name, response.ok ? await response.text() : null);
        } catch {
          wikiUrls.set(name, null);
        }
      }));
      poke();
    }, 250);
    return undefined;
  }

  async function follow(link) {
    if (link.wiki === undefined) {
      location.href = new URL(link.href, location.href).href;
      return;
    }
    let url = wikiUrls.get(link.wiki);
    if (url === undefined) {
      const response = await fetch(location.pathname + "?wiki=" + encodeURIComponent(link.wiki), { cache: "no-store" });
      url = response.ok ? await response.text() : null;
      wikiUrls.set(link.wiki, url);
    }
    if (url) location.href = url;
    // As in the reading view: a link to a note that does not exist creates it.
    else window.__visorPost({ type: "create_note", dir: config.dir, name: link.wiki });
  }

  const unbracket = (url) => (url.startsWith("<") && url.endsWith(">") ? url.slice(1, -1) : url);

  // The link at `pos`, if any: { href } or { wiki }.
  // Enter continues a list item, as in Obsidian; on an empty item it ends the list.
  function continueList(view) {
    const { state } = view;
    const sel = state.selection.main;
    if (!sel.empty) return false;
    const line = state.doc.lineAt(sel.head);
    const m = /^(\s*(?:>\s*)*)(?:([-*+])|(\d+)([.)]))(\s+)(\[[ xX]\]\s+)?/.exec(line.text);
    if (!m || sel.head - line.from < m[0].length) return false;
    for (let n = syntaxTree(state).resolveInner(sel.head, -1); n; n = n.parent) {
      if (/^(FencedCode|CodeBlock|FrontMatter|BlockMath)$/.test(n.name)) return false;
    }
    if (line.text.slice(m[0].length).trim() === "") {
      view.dispatch({ changes: { from: line.from, to: line.to, insert: m[1] }, selection: { anchor: line.from + m[1].length }, userEvent: "input" });
      return true;
    }
    const marker = m[2] || `${Number(m[3]) + 1}${m[4]}`;
    view.dispatch(state.replaceSelection("\n" + m[1] + marker + m[5] + (m[6] ? "[ ] " : "")), { scrollIntoView: true, userEvent: "input" });
    return true;
  }

  function linkAt(state, pos) {
    for (const side of [1, -1]) {
      for (let node = syntaxTree(state).resolveInner(pos, side); node; node = node.parent) {
        if (node.name === "WikiLink") {
          const target = node.getChild("WikiTarget");
          return target && { wiki: state.sliceDoc(target.from, target.to).trim() };
        }
        if (node.name === "Link" || node.name === "Image" || node.name === "Autolink") {
          const url = node.getChild("URL");
          return url && { href: unbracket(state.sliceDoc(url.from, url.to)) };
        }
        if (node.name === "URL") {
          const text = state.sliceDoc(node.from, node.to);
          return { href: /^[a-z][\w+.-]*:/i.test(text) ? text : (text.includes("@") ? "mailto:" : "https://") + text };
        }
      }
    }
    return null;
  }

  // ---------- widgets ----------

  // No widget may draw a replaced element (<img>, <input>, <canvas>…) inside the
  // editable, nor an element whose role WebKit treats as a leaf (role="img", a
  // <button>: role="figure" is fine): WebKitGTK 2.54 aborts the web process when a
  // selection lands at the end of the contenteditable while an AT-SPI client
  // watches (WebKit bug 279313). The DOM range counts the element as one
  // character (U+FFFC) that the accessible text omits, so the caret offset lands
  // one past the UTF-16 -> UTF-8 mapping and is indexed out of bounds.
  // CodeMirror's own widget buffer is the same case and is turned into a <span>
  // when the package is built (tools/codemirror).

  // A click on a rendered block puts the cursor in it, which shows its text.
  function reveal(view, dom, e) {
    const a = e.target.closest?.("a[data-href], a[data-wiki]");
    if (a && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      follow(a.dataset.wiki !== undefined ? { wiki: a.dataset.wiki } : { href: a.dataset.href });
      return;
    }
    if (e.target.closest?.("input")) return;
    e.preventDefault();
    view.dispatch({ selection: { anchor: view.posAtDOM(dom) } });
    view.focus();
  }

  class Label extends WidgetType {
    constructor(text, cls) { super(); this.text = text; this.cls = cls; }
    eq(other) { return other.text === this.text && other.cls === this.cls; }
    toDOM() {
      const span = document.createElement("span");
      span.className = this.cls;
      span.textContent = this.text;
      return span;
    }
  }

  class Check extends WidgetType {
    constructor(on) { super(); this.on = on; }
    eq(other) { return other.on === this.on; }
    toDOM(view) {
      // A <span>, not an <input>: the drawn box is all note.css styles it with.
      const box = document.createElement("span");
      box.className = "md-task";
      box.setAttribute("role", "checkbox");
      box.setAttribute("aria-checked", this.on ? "true" : "false");
      box.setAttribute("aria-label", this.on ? tx("Tarea hecha", "Task done") : tx("Tarea pendiente", "Task pending"));
      box.addEventListener("mousedown", (e) => {
        e.preventDefault();
        const at = view.posAtDOM(box) + 1;
        const mark = view.state.sliceDoc(at, at + 1);
        view.dispatch({ changes: { from: at, to: at + 1, insert: /x/i.test(mark) ? " " : "x" } });
      });
      box.addEventListener("click", (e) => e.preventDefault());
      return box;
    }
    ignoreEvent() { return true; }
  }

  class Formula extends WidgetType {
    constructor(tex, display) { super(); this.tex = tex; this.display = display; }
    eq(other) { return other.tex === this.tex && other.display === this.display; }
    toDOM(view) {
      const el = document.createElement(this.display ? "div" : "span");
      el.className = this.display ? "md-math-block" : "md-math";
      window.katex.render(this.tex, el, { displayMode: this.display, throwOnError: false });
      if (this.display) el.addEventListener("mousedown", (e) => reveal(view, el, e));
      return el;
    }
    get estimatedHeight() { return this.display ? 60 : -1; }
    // Inline, the editor places the cursor; a block handles its own click.
    ignoreEvent() { return this.display; }
  }

  // Pele's handle for each drawn diagram, so destroy() can stop it.
  const handles = new WeakMap();
  class Diagram extends WidgetType {
    constructor(source) { super(); this.source = source; }
    eq(other) { return other.source === this.source; }
    toDOM(view) {
      const el = document.createElement("div");
      el.className = "md-mermaid";
      el.addEventListener("mousedown", (e) => reveal(view, el, e));
      // No zoom: its controls are <button>s, which abort the web process inside
      // the editable like a replaced element (see the note above the widgets).
      handles.set(el, window.__visorNote.diagram(el, this.source, { zoom: false, onRender: () => view.requestMeasure() }));
      return el;
    }
    destroy(dom) {
      handles.get(dom)?.destroy();
      handles.delete(dom);
    }
    ignoreEvent() { return true; }
  }

  class Picture extends WidgetType {
    constructor(src, alt) { super(); this.src = src; this.alt = alt; }
    eq(other) { return other.src === this.src && other.alt === this.alt; }
    toDOM(view) {
      // The image goes as the box's background, not as an <img>: a replaced
      // element inside the editable aborts the web process (WebKit bug 279313).
      // The box takes the image's shape through its aspect ratio, and
      // editor.css sizes it like the reading view's images.
      const box = document.createElement("div");
      box.className = "md-image";
      box.setAttribute("role", "figure");
      if (this.alt) box.setAttribute("aria-label", this.alt);
      const url = new URL(this.src, location.href).href;
      box.style.backgroundImage = `url("${url.replace(/"/g, "%22")}")`;
      const size = new Image();
      size.onload = () => {
        box.style.width = size.naturalWidth + "px";
        box.style.aspectRatio = size.naturalWidth + " / " + size.naturalHeight;
        view.requestMeasure();
      };
      size.onerror = () => { box.textContent = this.alt; view.requestMeasure(); };
      size.src = url;
      box.addEventListener("mousedown", (e) => reveal(view, box, e));
      return box;
    }
    ignoreEvent() { return true; }
  }

  // Inline Markdown of a table cell, as HTML elements, from its syntax tree.
  function inline(state, node, parent) {
    let at = node.from;
    const text = (to) => {
      if (to > at) parent.append(state.sliceDoc(at, to));
    };
    for (let child = node.firstChild; child; child = child.nextSibling) {
      text(child.from);
      at = child.to;
      const into = (tag) => {
        const el = parent.appendChild(document.createElement(tag));
        inline(state, child, el);
        return el;
      };
      switch (child.name) {
        case "Emphasis": into("em"); break;
        case "StrongEmphasis": into("strong"); break;
        case "Strikethrough": into("del"); break;
        case "InlineCode": into("code"); break;
        case "Escape": parent.append(state.sliceDoc(child.from + 1, child.to)); break;
        case "Link": case "Autolink": {
          const url = child.getChild("URL");
          const a = into("a");
          if (url) a.dataset.href = unbracket(state.sliceDoc(url.from, url.to));
          if (child.name === "Autolink" && url) a.textContent = a.dataset.href;
          break;
        }
        case "URL": {
          const a = parent.appendChild(document.createElement("a"));
          a.textContent = a.dataset.href = state.sliceDoc(child.from, child.to);
          break;
        }
        case "WikiLink": {
          const a = parent.appendChild(document.createElement("a"));
          const target = child.getChild("WikiTarget");
          const marks = child.getChildren("WikiMark");
          a.dataset.wiki = state.sliceDoc(target.from, target.to).trim();
          a.textContent = marks.length === 3 ? state.sliceDoc(marks[1].to, marks[2].from) : a.dataset.wiki.replace("#", " › ");
          break;
        }
        case "InlineMath": {
          const marks = child.getChildren("MathMark");
          const tex = state.sliceDoc(marks[0].to, marks[1].from);
          const span = parent.appendChild(document.createElement("span"));
          if (lib("katex")) window.katex.render(tex, span, { throwOnError: false });
          else span.textContent = tex;
          break;
        }
        case "Image": break;
        default:
          // Marks (* ` [ ] ( ) …) are not content; anything else keeps its text.
          if (!/Mark$|^LinkTitle$|^LinkLabel$/.test(child.name)) inline(state, child, parent);
      }
    }
    text(node.to);
  }

  class Table extends WidgetType {
    constructor(state, node) {
      super();
      this.source = state.sliceDoc(node.from, node.to);
      this.rows = [];
      for (let row = node.firstChild; row; row = row.nextSibling) {
        if (row.name !== "TableHeader" && row.name !== "TableRow") continue;
        const line = state.sliceDoc(row.from, row.to).trim();
        const cells = [];
        let cell = null;
        let started = !line.startsWith("|");
        for (let c = row.firstChild; c; c = c.nextSibling) {
          if (c.name === "TableDelimiter") {
            if (started) cells.push(cell);
            cell = null;
            started = true;
          } else if (c.name === "TableCell") cell = c;
        }
        if (cell || !line.endsWith("|")) cells.push(cell);
        this.rows.push({ head: row.name === "TableHeader", cells: cells.map((c) => c && { node: c }) });
      }
      const delimiter = state.doc.lineAt(node.from).number + 1;
      const spec = state.doc.line(Math.min(delimiter, state.doc.lines)).text.trim().replace(/^\||\|$/g, "");
      this.align = spec.split("|").map((s) => {
        s = s.trim();
        return s.startsWith(":") && s.endsWith(":") ? "center" : s.endsWith(":") ? "right" : s.startsWith(":") ? "left" : "";
      });
      this.state = state;
    }
    eq(other) { return other.source === this.source; }
    toDOM(view) {
      const wrap = document.createElement("div");
      wrap.className = "md-table";
      const table = wrap.appendChild(document.createElement("table"));
      const columns = this.rows[0]?.cells.length || 0;
      let body = null;
      for (const row of this.rows) {
        const section = row.head ? table.appendChild(document.createElement("thead")) : (body ||= table.appendChild(document.createElement("tbody")));
        const tr = section.appendChild(document.createElement("tr"));
        for (let i = 0; i < columns; i++) {
          const td = tr.appendChild(document.createElement(row.head ? "th" : "td"));
          if (this.align[i]) td.style.textAlign = this.align[i];
          const cell = row.cells[i];
          if (cell) inline(this.state, cell.node, td);
        }
      }
      wrap.addEventListener("mousedown", (e) => reveal(view, wrap, e));
      return wrap;
    }
    get estimatedHeight() { return this.rows.length * 38 + 16; }
    ignoreEvent() { return true; }
  }

  // ---------- which lines show their marks ----------

  // Line numbers that hold a cursor or selection.
  function activeLines(state) {
    const lines = [];
    for (const r of state.selection.ranges) lines.push([state.doc.lineAt(r.from).number, state.doc.lineAt(r.to).number]);
    return lines;
  }
  function touches(state, active, from, to) {
    const a = state.doc.lineAt(from).number;
    const z = state.doc.lineAt(to).number;
    return active.some(([s, e]) => s <= z && e >= a);
  }

  function fenceInfo(state, node) {
    const info = node.getChild("CodeInfo");
    return info ? state.sliceDoc(info.from, info.to).trim().split(/\s/)[0].toLowerCase() : "";
  }

  // ---------- block previews: tables, diagrams, display maths, images ----------
  // Widgets that replace whole lines must come from a state field.

  function findBlocks(state) {
    const blocks = [];
    syntaxTree(state).iterate({
      enter(node) {
        switch (node.name) {
          case "Table":
            blocks.push({ kind: "table", from: node.from, to: node.to, node: node.node });
            return false;
          case "FencedCode":
            if (fenceInfo(state, node.node) === "mermaid") blocks.push({ kind: "mermaid", from: node.from, to: node.to, node: node.node });
            return false;
          case "BlockMath":
            blocks.push({ kind: "math", from: node.from, to: node.to, node: node.node });
            return false;
          case "Image": {
            const url = node.node.getChild("URL");
            if (url) {
              const marks = node.node.getChildren("LinkMark");
              const alt = marks.length > 1 ? state.sliceDoc(marks[0].to, marks[1].from) : "";
              blocks.push({ kind: "image", from: node.from, to: node.to, src: unbracket(state.sliceDoc(url.from, url.to)), alt });
            }
            return false;
          }
          case "WikiEmbed": {
            const target = node.node.getChild("WikiTarget");
            const name = state.sliceDoc(target.from, target.to).trim();
            // Rust answers with a redirect to the image, wherever it is.
            if (/\.(png|jpe?g|gif|webp|avif|svg|ico)$/i.test(name)) {
              blocks.push({ kind: "image", from: node.from, to: node.to, src: location.pathname + "?embed=" + encodeURIComponent(name), alt: name });
            }
            return false;
          }
          case "CodeBlock": case "HTMLBlock": case "FrontMatter": case "CommentBlock":
            return false;
        }
        return undefined;
      },
    });
    for (const b of blocks) {
      b.first = state.doc.lineAt(b.from).number;
      b.last = state.doc.lineAt(b.to).number;
    }
    return blocks;
  }

  function blockDecorations(state, blocks) {
    const active = activeLines(state);
    const out = [];
    for (const b of blocks) {
      const start = state.doc.lineAt(b.from);
      const end = state.doc.lineAt(b.to);
      if (b.kind === "image") {
        const widget = new Picture(b.src, b.alt);
        // Alone on its line, the image takes the line's place, as when reading.
        const alone = start.number === end.number && start.text.trim() === state.sliceDoc(b.from, b.to);
        if (alone && !touches(state, active, b.from, b.to)) out.push(Decoration.replace({ widget, block: true }).range(start.from, end.to));
        else out.push(Decoration.widget({ widget, block: true, side: 1 }).range(end.to));
        continue;
      }
      if (touches(state, active, b.from, b.to)) continue;
      let widget = null;
      if (b.kind === "table") widget = new Table(state, b.node);
      else if (b.kind === "mermaid" && lib("pele")) {
        const closed = end.number > start.number && /^\s*(```|~~~)/.test(end.text);
        const last = closed ? end.number - 1 : end.number;
        const body = last > start.number ? state.doc.sliceString(start.to + 1, state.doc.line(last).to) : "";
        widget = new Diagram(body);
      } else if (b.kind === "math" && lib("katex")) {
        const marks = b.node.getChildren("MathMark");
        const tex = marks.length > 1 ? state.sliceDoc(marks[0].to, marks[1].from) : state.sliceDoc(marks[0].to, b.to);
        widget = new Formula(tex.trim(), true);
      }
      if (widget) out.push(Decoration.replace({ widget, block: true }).range(start.from, end.to));
    }
    return Decoration.set(out, true);
  }

  // Which blocks the cursor is in. Moving it inside a block, or between plain lines,
  // changes nothing on screen, so the widgets are not rebuilt then.
  function touching(state, blocks) {
    const active = activeLines(state);
    let key = "";
    for (let i = 0; i < blocks.length; i++) {
      const b = blocks[i];
      if (active.some(([s, e]) => s <= b.last && e >= b.first)) key += i + ",";
    }
    return key;
  }

  const blockField = StateField.define({
    create(state) {
      const blocks = findBlocks(state);
      return { blocks, touch: touching(state, blocks), deco: blockDecorations(state, blocks) };
    },
    update(value, tr) {
      const tree = syntaxTree(tr.state) !== syntaxTree(tr.startState);
      const libs = tr.effects.some((e) => e.is(refresh));
      if (tr.docChanged || tree || libs) {
        const blocks = findBlocks(tr.state);
        return { blocks, touch: touching(tr.state, blocks), deco: blockDecorations(tr.state, blocks) };
      }
      if (tr.selection) {
        const touch = touching(tr.state, value.blocks);
        if (touch === value.touch) return value;
        return { blocks: value.blocks, touch, deco: blockDecorations(tr.state, value.blocks) };
      }
      return value;
    },
    provide: (field) => EditorView.decorations.from(field, (value) => value.deco),
  });

  // ---------- inline preview, only for what is on screen ----------

  const hide = Decoration.replace({});
  const lineClass = (cls) => Decoration.line({ class: cls });
  const hljsCache = new Map();

  // highlight.js output as [from, to, class] offsets into `code`.
  function hljsRanges(code, language) {
    const key = language + "\n" + code;
    let ranges = hljsCache.get(key);
    if (ranges) return ranges;
    ranges = [];
    const html = window.hljs.highlight(code, { language, ignoreIllegals: true }).value;
    const tpl = document.createElement("template");
    tpl.innerHTML = html;
    let at = 0;
    const walk = (el) => {
      for (const child of el.childNodes) {
        if (child.nodeType === 3) at += child.data.length;
        else {
          const from = at;
          walk(child);
          const cls = child.className.split(" ").filter((c) => c.startsWith("hljs-")).join(" ");
          if (cls && at > from) ranges.push([from, at, cls]);
        }
      }
    };
    walk(tpl.content);
    if (hljsCache.size > 200) hljsCache.clear();
    hljsCache.set(key, ranges);
    return ranges;
  }

  function inlineDecorations(view) {
    const { state } = view;
    const doc = state.doc;
    const active = activeLines(state);
    const on = (from, to = from) => touches(state, active, from, to);
    const out = [];
    const sameLine = (from, to) => doc.lineAt(from).number === doc.lineAt(to).number;
    const conceal = (from, to, deco = hide) => {
      if (to > from && sameLine(from, to)) out.push(deco.range(from, to));
    };
    // A class on each line from `from` to `to`, only for those in the viewport.
    const eachLine = (from, to, cls) => {
      const first = doc.lineAt(from).number;
      const last = doc.lineAt(to).number;
      const top = doc.lineAt(Math.max(from, view.viewport.from)).number;
      const bottom = doc.lineAt(Math.min(Math.max(to, from), view.viewport.to)).number;
      for (let n = top; n <= bottom; n++) {
        out.push(lineClass(typeof cls === "function" ? cls(n, first, last) : cls).range(doc.line(n).from));
      }
    };
    const replacedBlocks = state.field(blockField).deco;
    const replaced = (from) => {
      let hit = false;
      replacedBlocks.between(from, from, (f, t, d) => { if (d.spec.block && t > f && from >= f && from <= t) hit = true; });
      return hit;
    };

    {
      // The viewport, not visibleRanges: a block across two ranges would get its decorations twice.
      const { from, to } = view.viewport;
      syntaxTree(state).iterate({
        from, to,
        enter(ref) {
          const node = ref.node;
          const name = ref.name;
          if (/^(Table|FencedCode|BlockMath)$/.test(name) && replaced(ref.from)) return false;
          switch (name) {
            case "ATXHeading1": case "ATXHeading2": case "ATXHeading3":
            case "ATXHeading4": case "ATXHeading5": case "ATXHeading6":
              eachLine(ref.from, ref.from, "cm-h" + name.slice(-1));
              break;
            case "SetextHeading1": case "SetextHeading2": {
              const underline = doc.lineAt(ref.to);
              eachLine(ref.from, underline.from - 1, "cm-h" + name.slice(-1));
              if (!on(ref.from, ref.to)) eachLine(underline.from, underline.from, "cm-setext");
              break;
            }
            case "HeaderMark": {
              if (on(ref.from)) break;
              const heading = node.parent;
              if (heading && heading.name.startsWith("SetextHeading")) conceal(ref.from, ref.to);
              else if (ref.from === heading?.from) conceal(ref.from, Math.min(ref.to + 1, heading.to));
              else conceal(ref.from - 1 >= heading.from && state.sliceDoc(ref.from - 1, ref.from) === " " ? ref.from - 1 : ref.from, ref.to);
              break;
            }
            case "EmphasisMark": case "StrikethroughMark":
              if (!on(ref.from)) conceal(ref.from, ref.to);
              break;
            case "CodeMark":
              if (node.parent?.name === "InlineCode" && !on(ref.from)) conceal(ref.from, ref.to);
              break;
            case "Link": {
              if (on(ref.from, ref.to)) break;
              const marks = node.getChildren("LinkMark");
              // "[" and everything from "]" on: "](url "title")" or "][ref]".
              if (marks.length > 1) {
                conceal(marks[0].from, marks[0].to);
                conceal(marks[1].from, ref.to);
              }
              break;
            }
            case "Autolink":
              if (!on(ref.from)) for (const m of node.getChildren("LinkMark")) conceal(m.from, m.to);
              return false;
            case "Image":
              if (!on(ref.from, ref.to)) conceal(ref.from, ref.to);
              return false;
            case "WikiEmbed": {
              const target = node.getChild("WikiTarget");
              const image = /\.(png|jpe?g|gif|webp|avif|svg|ico)$/i.test(state.sliceDoc(target.from, target.to).trim());
              if (image && !on(ref.from, ref.to)) conceal(ref.from, ref.to);
              return false;
            }
            case "WikiLink": {
              const target = node.getChild("WikiTarget");
              const marks = node.getChildren("WikiMark");
              const name = state.sliceDoc(target.from, target.to).trim();
              const url = wikiUrl(name);
              out.push(Decoration.mark({ class: url === null ? "md-wiki md-wiki-new" : "md-wiki", attributes: { title: url === null ? tx(`Crear la nota «${name}» (Ctrl+clic)`, `Create note «${name}» (Ctrl+click)`) : tx("Ctrl+clic para abrir", "Ctrl+click to open") } }).range(ref.from, ref.to));
              if (on(ref.from)) return false;
              conceal(marks[0].from, marks[0].to);
              if (marks.length === 3) conceal(target.from, marks[1].to);
              else {
                const hash = state.sliceDoc(target.from, target.to).indexOf("#");
                if (hash > 0) conceal(target.from + hash, target.from + hash + 1, Decoration.replace({ widget: new Label(" › ", "md-wiki-sep") }));
              }
              conceal(marks[marks.length - 1].from, marks[marks.length - 1].to);
              return false;
            }
            case "InlineMath": {
              if (on(ref.from) || !sameLine(ref.from, ref.to)) return false;
              if (!lib("katex")) return false;
              const marks = node.getChildren("MathMark");
              const tex = state.sliceDoc(marks[0].to, marks[1].from);
              out.push(Decoration.replace({ widget: new Formula(tex, marks[0].to - marks[0].from === 2) }).range(ref.from, ref.to));
              return false;
            }
            case "TaskMarker": {
              const done = /x/i.test(state.sliceDoc(ref.from + 1, ref.from + 2));
              out.push(Decoration.replace({ widget: new Check(done) }).range(ref.from, ref.to));
              break;
            }
            case "ListMark": {
              // Set before the cursor check: the indent must stay when the cursor is on the marker.
              let depth = 0;
              for (let p = node.parent; p; p = p.parent) if (p.name === "BulletList" || p.name === "OrderedList") depth++;
              if (depth > 1) out.push(Decoration.line({ attributes: { style: "--depth: " + depth } }).range(doc.lineAt(ref.from).from));
              if (on(ref.from)) break;
              const list = node.parent?.parent;
              if (list?.name !== "BulletList" && list?.name !== "OrderedList") break;
              // Nested markers also hide the indent spaces before them: the line's padding gives the depth.
              const start = depth > 1 ? doc.lineAt(ref.from).from : ref.from;
              const end = Math.min(ref.to + 1, doc.lineAt(ref.from).to);
              if (list.name === "OrderedList") conceal(start, end, Decoration.replace({ widget: new Label(state.sliceDoc(ref.from, ref.to), "md-ordinal") }));
              else if (node.parent.getChild("Task")) conceal(start, end);
              else conceal(start, end, Decoration.replace({ widget: new Label("•", "md-bullet") }));
              break;
            }
            case "BulletList": case "OrderedList":
              if (node.parent?.name !== "ListItem") eachLine(ref.from, ref.to, "cm-list");
              break;
            case "Blockquote":
              eachLine(ref.from, ref.to, "cm-quote");
              break;
            case "QuoteMark":
              if (!on(ref.from)) conceal(ref.from, state.sliceDoc(ref.to, ref.to + 1) === " " ? ref.to + 1 : ref.to);
              break;
            case "HorizontalRule":
              if (!on(ref.from)) {
                eachLine(ref.from, ref.from, "cm-hr");
                conceal(ref.from, ref.to);
              }
              return false;
            case "FencedCode": {
              const blockOn = on(ref.from, ref.to);
              eachLine(ref.from, ref.to, (n, first, last) =>
                "cm-codeblock" + (n === first ? " cm-codeblock-first" : "") + (n === last ? " cm-codeblock-last" : ""));
              const marks = node.getChildren("CodeMark");
              if (!blockOn) {
                const info = node.getChild("CodeInfo");
                for (const m of marks) conceal(m.from, m.to);
                if (info) conceal(info.from, info.to, Decoration.replace({ widget: new Label(state.sliceDoc(info.from, info.to), "md-lang") }));
              }
              const language = fenceInfo(state, node);
              if (language && language !== "mermaid") {
                const hljs = lib("hljs");
                const open = doc.lineAt(ref.from);
                const last = doc.lineAt(ref.to);
                const closing = marks.length > 1 && last.number > open.number;
                const codeFrom = Math.min(open.to + 1, ref.to);
                const codeTo = closing ? Math.max(codeFrom, last.from - 1) : ref.to;
                if (hljs && hljs.getLanguage(language) && codeTo > codeFrom) {
                  const code = state.sliceDoc(codeFrom, codeTo);
                  for (const [a, b, cls] of hljsRanges(code, language)) {
                    const f = codeFrom + a;
                    const z = codeFrom + b;
                    if (z >= view.viewport.from && f <= view.viewport.to) out.push(Decoration.mark({ class: cls }).range(f, z));
                  }
                }
              }
              return false;
            }
            case "BlockMath":
              eachLine(ref.from, ref.to, "cm-mathsrc");
              return false;
            case "Table":
              eachLine(ref.from, ref.to, "cm-tablesrc");
              return false;
            case "FrontMatter":
              eachLine(ref.from, ref.to, "cm-frontmatter");
              return false;
          }
          return undefined;
        },
      });
    }
    return Decoration.set(out, true);
  }

  const preview = ViewPlugin.fromClass(
    class {
      constructor(view) { this.decorations = inlineDecorations(view); }
      update(u) {
        const tree = syntaxTree(u.state) !== syntaxTree(u.startState);
        if (u.docChanged || u.selectionSet || u.viewportChanged || tree || u.transactions.some((tr) => tr.effects.some((e) => e.is(refresh)))) {
          this.decorations = inlineDecorations(u.view);
        }
      }
    },
    { decorations: (v) => v.decorations },
  );

  // Up and down arrows would jump over a drawn block; they enter it instead,
  // which shows its text.
  const intoBlock = (forward) => (view) => {
    const { state } = view;
    const sel = state.selection.main;
    if (!sel.empty) return false;
    const line = state.doc.lineAt(sel.head);
    const next = forward ? line.to + 1 : line.from - 1;
    if (next < 0 || next > state.doc.length) return false;
    let block = null;
    state.field(blockField).deco.between(next, next, (from, to, deco) => {
      if (deco.spec.block && to > from) block = { from, to };
    });
    if (!block) return false;
    view.dispatch({ selection: { anchor: forward ? block.from : block.to }, scrollIntoView: true });
    return true;
  };

  // Ctrl+click follows a link, as a plain click does in the reading view.
  const clicks = EditorView.domEventHandlers({
    mousedown(e, view) {
      if (!(e.ctrlKey || e.metaKey) || e.button !== 0) return false;
      const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
      const link = pos === null ? null : linkAt(view.state, pos);
      if (!link) return false;
      e.preventDefault();
      follow(link);
      return true;
    },
  });

  // ---------- the editor ----------

  window.__visorEditor = {
    create(parent, text, hooks) {
      const view = new EditorView({
        parent,
        state: EditorState.create({
          doc: text,
          extensions: [
            history(),
            drawSelection(),
            EditorView.lineWrapping,
            EditorState.tabSize.of(4),
            EditorView.contentAttributes.of({ spellcheck: "false", autocorrect: "off", autocapitalize: "off", "aria-label": tx("Texto de la nota", "Note text") }),
            Prec.high(keymap.of([
              { key: "Mod-s", run: () => { hooks.save(); return true; } },
              { key: "Enter", run: continueList },
              { key: "Tab", run: insertTab },
              { key: "ArrowDown", run: intoBlock(true) },
              { key: "ArrowUp", run: intoBlock(false) },
            ])),
            keymap.of([...defaultKeymap, ...historyKeymap]),
            markdown,
            syntaxHighlighting(highlight),
            blockField,
            preview,
            clicks,
            EditorView.updateListener.of((u) => { if (u.docChanged) hooks.input(); }),
          ],
        }),
      });
      current = view;
      const doc = () => view.state.doc;

      // Scrolls so that the top of `pos`'s line sits `y` pixels from the top of
      // the window. Heights below are estimates until drawn: correct a few times.
      const scrollTo = (pos, y) => new Promise((done) => {
        let tries = 0;
        const step = () => view.requestMeasure({
          read: () => view.documentTop + view.lineBlockAt(pos).top - y,
          write: (delta) => {
            const before = view.scrollDOM.scrollTop;
            view.scrollDOM.scrollTop += delta;
            // Stop when on target or when the scroll cannot go further.
            if (Math.abs(delta) < 1 || view.scrollDOM.scrollTop === before || ++tries >= 5) done();
            else step();
          },
        });
        step();
      });

      return {
        text: () => doc().toString(),
        focus: () => view.focus(),
        get scrollTop() { return view.scrollDOM.scrollTop; },
        atEnd() {
          const end = doc().length;
          view.dispatch({ selection: { anchor: end }, effects: EditorView.scrollIntoView(end, { y: "center" }) });
        },
        // Opens on the source line the reader had at the top, at the same height.
        // The cursor goes to the first line wholly on screen.
        place({ line, y }) {
          const n = Math.max(1, Math.min(line, doc().lines));
          const caret = y < 0 && n < doc().lines ? n + 1 : n;
          view.dispatch({ selection: { anchor: doc().line(caret).from } });
          return scrollTo(doc().line(n).from, y);
        },
        // The line to show on returning to the view: the cursor's if the person
        // typed and it is on screen, otherwise the one at the top.
        anchor(typed) {
          const box = view.scrollDOM.getBoundingClientRect();
          if (typed) {
            const head = view.state.selection.main.head;
            const top = view.documentTop + view.lineBlockAt(head).top;
            if (top >= box.top && top < box.bottom) return { line: doc().lineAt(head).number, y: top };
          }
          const block = view.lineBlockAtHeight(box.top - view.documentTop);
          return { line: doc().lineAt(block.from).number, y: view.documentTop + block.top };
        },
      };
    },
  };
})();
