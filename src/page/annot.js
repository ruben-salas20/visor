// Annotation layer for vault pages: text highlights and notes pinned to
// elements, drawn over the page without touching its DOM. Highlights use the
// CSS Custom Highlight API; markers, toolbars and dialogs live in a shadow root
// attached to <html>, outside <body>, so the page's own text never includes them.
(() => {
  const tr = (es, en) => ("__VISOR_LANG__" === "en" ? en : es);
  const BASE = "__VISOR_BASE__";
  if (window.top !== window || window.__visorAnnot || !location.href.startsWith(BASE)) return;

  const page = decodeURIComponent(location.href.slice(BASE.length).split(/[?#]/)[0]);
  // A page opened on its own (an absolute path) has no vault to keep notes in.
  if (page.startsWith("/")) return;
  // A drawing has no text to mark: its canvas is Excalidraw's.
  if (/\.excalidraw(\.md)?$/i.test(page)) return;
  // A PDF is WebKit's viewer, whose own tools take the selection; PDF notes are #27.
  if (/\.pdf$/i.test(page)) return;
  const storeUrl = BASE + ".visor/anotaciones/" + page.split("/").map(encodeURIComponent).join("/") + ".json";
  const send = (m) => window.__visorPost(m);
  const COLORS = { amarillo: "#f2c94c", verde: "#6fcf97", azul: "#56a0f5", rosa: "#f28bb0" };
  const COLOR_NAMES = { amarillo: tr("amarillo", "yellow"), verde: tr("verde", "green"), azul: tr("azul", "blue"), rosa: tr("rosa", "pink") };
  const SKIP = "script,style,noscript,textarea,template,[data-visor-skip]";
  const CONTEXT = 32;

  // Cuts by code points, not UTF-16 units: a cut inside an emoji leaves a lone
  // surrogate, and visor refuses the whole message that carries one.
  const cut = (s, max) => {
    const chars = Array.from(s);
    return chars.length > max ? chars.slice(0, max).join("") : s;
  };
  // A slice of the page text may start or end inside a pair: drop the stray half.
  const whole = (s) => s.replace(/^[\uDC00-\uDFFF]/, "").replace(/[\uD800-\uDBFF]$/, "").toWellFormed();

  let notes = [];
  const anchors = new Map(); // id -> Range (text) or { el, point } (element)
  let host = null;
  let ui = null;
  let lastReport = "";

  // ---------- text index: the page's visible text with a map back to nodes ----------

  function textIndex() {
    const nodes = [];
    let text = "";
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
      acceptNode: (n) => (n.parentElement && !n.parentElement.closest(SKIP) ? 1 : 3),
    });
    for (let n; (n = walker.nextNode()); ) {
      nodes.push({ node: n, start: text.length });
      text += n.data;
    }
    return { text, nodes };
  }

  function offsetOf(index, container, offset) {
    if (container.nodeType === Node.TEXT_NODE) {
      const entry = index.nodes.find((e) => e.node === container);
      if (entry) return entry.start + offset;
    }
    const probe = document.createRange();
    probe.setStart(container, offset);
    for (const e of index.nodes) if (probe.comparePoint(e.node, 0) >= 0) return e.start;
    return index.text.length;
  }

  function rangeAt(index, start, end) {
    const point = (offset) => {
      let lo = 0, hi = index.nodes.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (index.nodes[mid].start <= offset) lo = mid; else hi = mid - 1;
      }
      const e = index.nodes[lo];
      return [e.node, Math.min(offset - e.start, e.node.data.length)];
    };
    const range = document.createRange();
    range.setStart(...point(start));
    range.setEnd(...point(end));
    return range;
  }

  // Finds the quote again: the occurrence whose surroundings match best,
  // and among equals the one closest to where it used to be.
  function locate(index, quote) {
    const { exact, prefix = "", suffix = "", start = 0 } = quote;
    if (!exact) return null;
    let best = null, bestScore = -Infinity;
    for (let i = index.text.indexOf(exact); i !== -1; i = index.text.indexOf(exact, i + 1)) {
      const before = index.text.slice(Math.max(0, i - prefix.length), i);
      const after = index.text.slice(i + exact.length, i + exact.length + suffix.length);
      let score = 0;
      while (score < before.length && before[before.length - 1 - score] === prefix[prefix.length - 1 - score]) score++;
      let s = 0;
      while (s < after.length && after[s] === suffix[s]) s++;
      score += s - Math.abs(i - start) / 1e6;
      if (score > bestScore) { best = i; bestScore = score; }
    }
    return best === null ? null : rangeAt(index, best, best + exact.length);
  }

  function quoteOf(index, range) {
    const start = offsetOf(index, range.startContainer, range.startOffset);
    const end = offsetOf(index, range.endContainer, range.endOffset);
    const exact = index.text.slice(start, end);
    return {
      exact: exact.toWellFormed(),
      prefix: whole(index.text.slice(Math.max(0, start - CONTEXT), start)),
      suffix: whole(index.text.slice(end, end + CONTEXT)),
      start,
    };
  }

  // ---------- elements ----------

  // Text a person can read: an inline <style> (Mermaid puts one first in each
  // diagram) would make the snippet as unstable as the ids inside it.
  function snippet(el) {
    let text = "";
    const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    for (let n; (n = walk.nextNode()) && text.length < 400; ) {
      if (!n.parentElement?.closest("style,script")) text += n.data + " ";
    }
    return cut(text.replace(/\s+/g, " ").trim(), 80);
  }

  // Ids built from a timestamp, like Mermaid's "mermaid-1791323458942", change on
  // every render, so they cannot find the element again.
  const generated = (id) => /\d{10,}/.test(id);

  function describe(el) {
    const path = [];
    for (let n = el; n && n !== document.body && n.nodeType === 1; n = n.parentElement) {
      if (n.id && !generated(n.id) && document.querySelectorAll("#" + CSS.escape(n.id)).length === 1) {
        path.unshift("#" + CSS.escape(n.id));
        break;
      }
      let i = 1;
      for (let s = n.previousElementSibling; s; s = s.previousElementSibling) if (s.tagName === n.tagName) i++;
      path.unshift(`${n.tagName.toLowerCase()}:nth-of-type(${i})`);
    }
    if (!path[0].startsWith("#")) path.unshift("body");
    return {
      css: path.join(" > "),
      tag: el.tagName.toLowerCase(),
      id: (el.id && !generated(el.id)) ? el.id : null,
      text: snippet(el),
      src: el.getAttribute("src") || el.getAttribute("href") || null,
    };
  }

  function findElement(d) {
    if (d.id) {
      const el = document.getElementById(d.id);
      if (el) return el;
    }
    try {
      const el = document.querySelector(d.css);
      if (el && el.tagName.toLowerCase() === d.tag) return el;
    } catch {}
    for (const el of document.getElementsByTagName(d.tag)) {
      if (d.src && (el.getAttribute("src") || el.getAttribute("href")) === d.src) return el;
      if (d.text && snippet(el) === d.text) return el;
    }
    // Notes saved before generated ids were skipped kept only that id. If the page
    // has a single element of that kind with a fresh id, it is the same one.
    if (d.id && generated(d.id)) {
      const prefix = d.id.slice(0, d.id.search(/\d{10,}/));
      const same = [...document.getElementsByTagName(d.tag)].filter((el) => generated(el.id) && el.id.startsWith(prefix));
      if (same.length === 1) return same[0];
    }
    return null;
  }

  // ---------- titles for element notes ----------

  const TITLE_MAX = 120;
  const clip = (s) => (Array.from(s).length > TITLE_MAX ? cut(s, TITLE_MAX - 1).trimEnd() + "…" : s);
  const clean = (s) => (s || "").replace(/\s+/g, " ").trim();

  // What a person would call the element, never its tag name.
  function kindOf(tag, el) {
    if (el?.closest?.("table")) return tr("Tabla", "Table");
    if (el?.closest?.("pre")) return tr("Código", "Code");
    if (el?.closest?.(".katex,math,mjx-container")) return tr("Fórmula", "Formula");
    if (el instanceof SVGElement) return outerSvg(el).getAttribute("role") === "img" ? tr("Imagen", "Image") : tr("Diagrama", "Diagram");
    const kinds = {
      img: tr("Imagen", "Image"), picture: tr("Imagen", "Image"), figure: tr("Figura", "Figure"), canvas: tr("Diagrama", "Diagram"), svg: tr("Diagrama", "Diagram"),
      video: tr("Vídeo", "Video"), audio: tr("Audio", "Audio"), iframe: tr("Contenido incrustado", "Embedded content"), embed: tr("Contenido incrustado", "Embedded content"), object: tr("Contenido incrustado", "Embedded content"),
      table: tr("Tabla", "Table"), caption: tr("Tabla", "Table"), thead: tr("Tabla", "Table"), tbody: tr("Tabla", "Table"), tr: tr("Tabla", "Table"), td: tr("Tabla", "Table"), th: tr("Tabla", "Table"),
      pre: tr("Código", "Code"), code: tr("Código", "Code"), math: tr("Fórmula", "Formula"),
      ul: tr("Lista", "List"), ol: tr("Lista", "List"), li: tr("Lista", "List"), dl: tr("Lista", "List"),
      p: tr("Párrafo", "Paragraph"), blockquote: tr("Cita", "Quote"), a: tr("Enlace", "Link"),
      button: tr("Control", "Control"), input: tr("Control", "Control"), select: tr("Control", "Control"), textarea: tr("Control", "Control"), label: tr("Control", "Control"),
      h1: tr("Encabezado", "Heading"), h2: tr("Encabezado", "Heading"), h3: tr("Encabezado", "Heading"), h4: tr("Encabezado", "Heading"), h5: tr("Encabezado", "Heading"), h6: tr("Encabezado", "Heading"),
    };
    return kinds[tag] || tr("Elemento", "Element");
  }

  function outerSvg(el) {
    while (el.ownerSVGElement) el = el.ownerSVGElement;
    return el;
  }

  // The heading of the section the element is in: the last one before it.
  function sectionOf(el) {
    let found = null;
    for (const h of document.body.querySelectorAll("h1,h2,h3,h4,h5,h6")) {
      if (h.contains(el)) return clean(h.textContent);
      if (h.compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING) found = h;
      else break;
    }
    return found ? clean(found.textContent) : "";
  }

  // Clicks land on the deepest element (a cell, a path of an SVG); its label
  // usually lives on the table, figure or drawing around it.
  function defaultTitle(el) {
    const candidates = [el];
    if (el instanceof SVGElement) candidates.push(outerSvg(el));
    const box = el.closest("table,figure,picture");
    if (box) candidates.push(box);
    for (const c of candidates) {
      for (const attr of ["aria-label", "title", "alt"]) {
        const v = clean(c.getAttribute(attr));
        if (v) return clip(v);
      }
    }
    const caption = el.closest("figure")?.querySelector("figcaption") || el.closest("table")?.caption;
    if (caption && clean(caption.textContent)) return clip(clean(caption.textContent));
    const kind = kindOf(el.tagName.toLowerCase(), el);
    if (kind === tr("Encabezado", "Heading")) return clip(tr(`Encabezado “${clean(el.textContent)}”`, `Heading “${clean(el.textContent)}”`));
    const section = sectionOf(el);
    return clip(section ? tr(`${kind} en “${section}”`, `${kind} in “${section}”`) : kind);
  }

  // Title of an element note: the saved one, or for notes saved before titles
  // existed, one worked out from the element if it is still on the page.
  function titleOf(note) {
    if (note.title) return note.title;
    const anchor = anchors.get(note.id);
    if (anchor && !(anchor instanceof Range)) return defaultTitle(anchor.el);
    const d = note.target.element || {};
    return kindOf(d.tag || "") + (d.text ? ": " + clip(d.text) : "");
  }

  // ---------- drawing ----------

  const CSS_TEXT = `
    :host { all: initial; }
    * { box-sizing: border-box; }
    /* Theme muted, red and accent often fall below 4.5:1 on the theme background;
       mixing them with the text color keeps small text readable in every theme. */
    .ui { font: 13px/1.45 system-ui, sans-serif; color: var(--visor-fg, #222); color-scheme: var(--visor-mode, light dark);
      --soft: color-mix(in srgb, var(--visor-fg, #222) 80%, var(--visor-bg, #fff));
      --danger: color-mix(in srgb, var(--visor-red, #c0392b) 60%, var(--visor-fg, #222));
      --strong: color-mix(in srgb, var(--visor-accent, #1a6cc4) 65%, var(--visor-fg, #222));
      /* Transparent rings instead of opaque borders: the layer sits over pages of any colour. */
      --ring: 0 0 0 1px light-dark(oklch(0 0 0 / .06), oklch(1 0 0 / .08)), 0 1px 2px -1px light-dark(oklch(0 0 0 / .06), transparent), 0 2px 4px 0 light-dark(oklch(0 0 0 / .04), transparent);
      --ring-hover: 0 0 0 1px light-dark(oklch(0 0 0 / .08), oklch(1 0 0 / .13)), 0 1px 2px -1px light-dark(oklch(0 0 0 / .08), transparent), 0 2px 4px 0 light-dark(oklch(0 0 0 / .06), transparent); }
    .card { background: color-mix(in srgb, var(--visor-fg, #222) 4%, var(--visor-bg, #fff)); color: var(--visor-fg, #222);
      border-radius: 10px; z-index: 1;
      box-shadow: 0 0 0 1px light-dark(oklch(0 0 0 / .1), oklch(1 0 0 / .12)), 0 2px 6px rgb(0 0 0 / .18), 0 12px 32px rgb(0 0 0 / .28); }
    button { font: inherit; color: inherit; background: none; border: 0; cursor: pointer; border-radius: 6px; }
    button:focus-visible, textarea:focus-visible, input:focus-visible { outline: 2px solid var(--visor-accent, #1a6cc4); outline-offset: 1px; }
    .pin { position: absolute; width: 22px; height: 22px; margin: -11px 0 0 -11px; border-radius: 50%;
      display: grid; place-items: center; padding: 0; pointer-events: auto; cursor: pointer;
      border: 2px solid var(--visor-bg, #fff); box-shadow: 0 1px 4px rgb(0 0 0 / .35); }
    .pin.text { width: 14px; height: 14px; margin: -7px 0 0 -3px; }
    .pin svg { display: block; width: 12px; height: 12px; }
    .toolbar { position: fixed; display: flex; gap: 2px; padding: 4px; pointer-events: auto; }
    .toolbar .dot { width: 26px; height: 26px; display: grid; place-items: center; }
    .toolbar .dot span { width: 16px; height: 16px; border-radius: 50%; }
    .toolbar .dot:hover, .toolbar .text:hover { background: color-mix(in srgb, var(--visor-fg, #222) 10%, transparent); }
    .toolbar .text { padding: 0 10px; }
    /* minmax(0, 1fr): without it the column grows to the unwrapped quote and the content spills out of the card. */
    .pop { position: fixed; width: min(340px, calc(100vw - 16px)); max-height: calc(100vh - 16px); overflow: auto;
      padding: 12px; border-radius: 18px; pointer-events: auto; display: grid; grid-template-columns: minmax(0, 1fr); gap: 8px; }
    .pop header { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; min-width: 0; }
    .quote { flex: 1 1 0; min-width: 0; display: flex; color: var(--soft); font-size: 12px; white-space: nowrap; }
    .quote span { overflow: hidden; text-overflow: ellipsis; }
    .name { flex: 1 1 14em; min-width: 0; font: inherit; font-weight: 600; color: inherit; padding: 4px 6px;
      background: transparent; border: 1px solid color-mix(in srgb, var(--visor-fg, #222) 18%, var(--visor-bg, #fff));
      border-radius: 6px; text-overflow: ellipsis; }
    .name:hover { border-color: color-mix(in srgb, var(--visor-fg, #222) 40%, var(--visor-bg, #fff)); }
    .name:focus { background: color-mix(in srgb, var(--visor-fg, #222) 5%, var(--visor-bg, #fff));
      border-color: color-mix(in srgb, var(--visor-fg, #222) 25%, var(--visor-bg, #fff)); }
    .swatches { flex: none; display: flex; gap: 2px; }
    .swatch { width: 22px; height: 22px; border-radius: 50%; border: 2px solid transparent; background-clip: content-box; padding: 2px; }
    .swatch[aria-pressed="true"] { border-color: var(--visor-fg, #222); }
    textarea { width: 100%; min-height: 84px; resize: vertical; font: inherit; color: inherit; padding: 8px;
      background: color-mix(in srgb, var(--visor-fg, #222) 5%, var(--visor-bg, #fff));
      border: 1px solid color-mix(in srgb, var(--visor-fg, #222) 25%, var(--visor-bg, #fff)); border-radius: 6px; }
    .replies { display: grid; gap: 6px; max-height: 260px; overflow: auto; }
    .reply { padding: 6px 8px; border-radius: 6px; white-space: pre-wrap;
      background: color-mix(in srgb, var(--visor-accent, #1a6cc4) 10%, var(--visor-bg, #fff)); }
    .reply.tu { margin-left: 24px; background: color-mix(in srgb, var(--visor-fg, #222) 7%, var(--visor-bg, #fff)); }
    .reply b { display: block; font-size: 11px; color: var(--soft); font-weight: 600; }
    .reply-form { display: grid; gap: 6px; }
    .reply-form[hidden] { display: none; }
    .reply-form textarea { min-height: 56px; }
    .reply-field { position: relative; }
    .reply-field textarea { display: block; padding-right: 52px; }
    .send { position: absolute; right: 18px; bottom: 6px; width: 28px; height: 28px; padding: 0; display: grid; place-items: center;
      color: var(--soft); border-radius: 4px; }
    .send svg { width: 16px; height: 16px; display: block; }
    .send:hover { color: var(--visor-fg, #222); background: color-mix(in srgb, var(--visor-fg, #222) 10%, transparent); }
    .reply-field.filled .send { color: var(--visor-bg, #fff); background: var(--strong); }
    .reply-field.filled .send:hover { filter: brightness(1.1); }
    .reply-err { margin: 0; color: var(--danger); font-size: 12px; }
    .reply-err:empty { display: none; }
    .unsaved { margin: 0; color: var(--danger); font-size: 12px; }
    .unsaved:empty { display: none; }
    .actions { display: flex; flex-wrap: wrap; gap: 6px; justify-content: flex-end; }
    .actions button { padding: 5px 10px; box-shadow: var(--ring);
      background: color-mix(in srgb, var(--visor-fg, #222) 4%, var(--visor-bg, #fff)); }
    .actions button:hover { background: color-mix(in srgb, var(--visor-fg, #222) 10%, var(--visor-bg, #fff)); box-shadow: var(--ring-hover); }
    .actions .primary, .actions .primary:hover { background: var(--strong); box-shadow: none;
      color: var(--visor-bg, #fff); font-weight: 600; }
    .actions .primary:hover { filter: brightness(1.1); }
    .actions .danger { color: var(--danger); margin-right: auto;
      box-shadow: 0 0 0 1px color-mix(in srgb, var(--danger) 50%, var(--visor-bg, #fff)); }
    /* Without this, the generic :hover rule above replaces the red ring on hover. */
    .actions .danger:hover { box-shadow: 0 0 0 1px var(--danger); }
    .hover { position: fixed; pointer-events: none; border: 2px solid var(--visor-accent, #1a6cc4); border-radius: 4px;
      background: color-mix(in srgb, var(--visor-accent, #1a6cc4) 12%, transparent); }
    .toast { position: fixed; bottom: 16px; left: 50%; translate: -50% 0; width: max-content; max-width: min(480px, calc(100vw - 24px));
      padding: 8px 12px; pointer-events: auto; display: flex; align-items: center; gap: 12px; color: var(--danger); }
    .toast span { flex: 1; min-width: 0; overflow-wrap: anywhere; }
    .toast button { flex: none; padding: 4px 8px; box-shadow: var(--ring); }
    .banner { position: fixed; top: 12px; left: 50%; translate: -50% 0; padding: 8px 14px; pointer-events: auto; }
    .panel { position: fixed; top: 12px; right: 12px; bottom: 12px; width: min(330px, calc(100vw - 24px)); padding: 12px; border-radius: 18px; pointer-events: auto;
      display: flex; flex-direction: column; gap: 8px; }
    .panel header { display: flex; align-items: center; font-weight: 600; }
    .panel header span { flex: 1; }
    .panel ul { list-style: none; margin: 0; padding: 0; overflow: auto; display: grid; gap: 6px; }
    .panel li button { width: 100%; text-align: left; padding: 8px; display: grid; gap: 2px;
      border-left: 4px solid var(--c); background: color-mix(in srgb, var(--visor-fg, #222) 4%, transparent); }
    .panel li button:hover { background: color-mix(in srgb, var(--visor-fg, #222) 9%, transparent); }
    .panel li strong { font-weight: 600; overflow-wrap: anywhere; }
    .panel q { font-style: italic; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; }
    .panel small { color: var(--soft); }
    .orphan { color: var(--danger); }
    .empty { color: var(--soft); }
    .actions button, .send, .toolbar .dot, .toolbar .text { transition-property: scale; transition-duration: 150ms; transition-timing-function: ease-out; }
    .actions button:active, .send:active, .toolbar .dot:active, .toolbar .text:active { scale: 0.96; }
  `;
  // Symmetric bubble with the tail in the middle, so it sits centred in the round marker.
  const ICON = `<svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2.5 2.5h7a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H7.2L6 10 4.8 8.5H2.5a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1z" fill="currentColor"/></svg>`;
  // Up arrow inside the reply field's corner button.
  const SEND = `<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 13V3.5M3.5 8 8 3.5 12.5 8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>`;

  function ensureHost() {
    if (host) return;
    host = document.createElement("visor-capa");
    host.style.cssText = "position:absolute;top:0;left:0;width:0;height:0;z-index:2147483647;pointer-events:none";
    const shadow = host.attachShadow({ mode: "open" });
    // A constructed sheet, not a <style>: pages with their own Content-Security-Policy
    // block inline styles unless they allow 'unsafe-inline', and the same goes for style="…"
    // attributes, so everything below sets styles through the CSSOM.
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(CSS_TEXT);
    shadow.adoptedStyleSheets = [sheet];
    ui = document.createElement("div");
    ui.className = "ui";
    shadow.append(ui);
    document.documentElement.append(host);
  }

  const highlightSheet = new CSSStyleSheet();
  highlightSheet.replaceSync(
    Object.entries(COLORS)
      .map(([name, c]) => `::highlight(visor-${name}) { background-color: color-mix(in srgb, ${c} 45%, transparent); }`)
      .join("\n"),
  );
  document.adoptedStyleSheets = [...document.adoptedStyleSheets, highlightSheet];

  // Finds every note in the page again and repaints. Cheap when nothing changed.
  function resolve() {
    const index = notes.some((n) => n.target.kind === "text") ? textIndex() : null;
    anchors.clear();
    for (const note of notes) {
      if (note.target.kind === "text") {
        const range = locate(index, note.target.quote || {});
        if (range) anchors.set(note.id, range);
      } else {
        const el = findElement(note.target.element || {});
        if (el) anchors.set(note.id, { el, point: note.target.point || { x: 0.5, y: 0.5 } });
      }
    }
    paint();
  }

  // Tells the app which notes could not be placed, for `visor notes`. Only on change.
  function report() {
    const saved = notes.filter((n) => n.saved);
    const orphans = saved.filter((n) => !anchors.has(n.id)).map((n) => n.id);
    const key = saved.length + ":" + orphans.join(",");
    if (key === lastReport) return;
    lastReport = key;
    send({ type: "annot_status", orphans, total: saved.length });
  }

  function paint() {
    for (const name of Object.keys(COLORS)) CSS.highlights.delete("visor-" + name);
    const groups = {};
    for (const note of notes) {
      const anchor = anchors.get(note.id);
      if (anchor instanceof Range) (groups[note.color] ||= new Highlight()).add(anchor);
    }
    for (const [name, h] of Object.entries(groups)) CSS.highlights.set("visor-" + name, h);
    placePins();
    report();
  }

  // Markers are kept per note id and touched only when they change: rebuilding them
  // on every pass recreated every node each second on pages whose DOM churns.
  const pins = new Map(); // note id -> { button, cls, left, top, color, title }

  function placePins() {
    const needPins = notes.some((n) => anchors.has(n.id) && (n.target.kind === "element" || n.comment || n.replies?.length));
    if (!needPins && !host) return;
    ensureHost();
    host.style.display = document.documentElement.dataset.visorEditing ? "none" : "";
    const placed = new Set();
    for (const note of notes) {
      const anchor = anchors.get(note.id);
      if (!anchor) continue;
      let x, y, cls;
      if (anchor instanceof Range) {
        if (!note.comment && !note.replies?.length) continue;
        const rects = anchor.getClientRects();
        const last = rects[rects.length - 1];
        if (!last) continue;
        x = last.right; y = last.top; cls = "pin text";
      } else {
        const r = anchor.el.getBoundingClientRect();
        if (!r.width && !r.height) continue;
        x = r.left + anchor.point.x * r.width; y = r.top + anchor.point.y * r.height; cls = "pin";
      }
      placed.add(note.id);
      placePin(note, cls, x + scrollX, y + scrollY);
    }
    for (const [id, pin] of pins) {
      if (placed.has(id)) continue;
      pin.button.remove();
      pins.delete(id);
    }
  }

  function placePin(note, cls, left, top) {
    const color = COLORS[note.color] || COLORS.amarillo;
    const title = note.comment || note.title || tr("Anotación", "Annotation");
    let pin = pins.get(note.id);
    if (!pin) {
      const button = document.createElement("button");
      button.style.color = "#222";
      button.setAttribute("aria-label", tr("Abrir anotación", "Open annotation"));
      const id = note.id;
      // Looked up at click time: a save or a reload replaces the note objects.
      button.onclick = (e) => {
        e.stopPropagation();
        const current = notes.find((n) => n.id === id);
        if (current) openNote(current);
      };
      pin = { button, cls: null, left: null, top: null, color: null, title: null };
      pins.set(id, pin);
      ui.append(button);
    }
    const { button } = pin;
    if (pin.cls !== cls) {
      button.className = cls;
      button.innerHTML = cls === "pin" ? ICON : "";
      pin.cls = cls;
    }
    if (pin.left !== left) { button.style.left = left + "px"; pin.left = left; }
    if (pin.top !== top) { button.style.top = top + "px"; pin.top = top; }
    if (pin.color !== color) { button.style.background = color; pin.color = color; }
    if (pin.title !== title) {
      button.title = title;
      // One name per marker: a page full of "Abrir anotación" tells a screen reader nothing.
      button.setAttribute("aria-label", tr("Abrir anotación: ", "Open annotation: ") + cut(title, 80));
      pin.title = title;
    }
  }

  // ---------- popover ----------

  let pop = null, popNote = null, conv = null, warned = false;
  // The last save failed: the text in the popover exists nowhere else.
  let failedSave = false;
  const MAX_COMMENT = 20000;
  const UNSAVED = tr("Tienes texto sin guardar. ", "You have unsaved text. ");
  const ESCAPE_AGAIN = tr("Pulsa Guardar para conservarlo, o Escape otra vez para descartarlo.", "Press Save to keep it, or Escape again to discard it.");
  // A new note that was never saved goes with its popover, whatever closed it.
  // Look it up in `notes`: after saving, `popNote` is still the unsaved copy.
  function closePop() {
    if (popNote && !notes.find((n) => n.id === popNote.id)?.saved) dropLocal(popNote.id);
    pop?.remove();
    pop = popNote = conv = null;
    warned = failedSave = false;
  }

  // Closing throws the typed text away, so the first try only warns when there is
  // text to lose. The second try always goes through.
  function closePopSafely(discard = closePop, again = tr("Pulsa Guardar para conservarlo, o repite lo que ibas a hacer para descartarlo.", "Press Save to keep it, or repeat what you were doing to discard it.")) {
    if (!pop) return true;
    if (hasUnsaved() && !warned) {
      warned = true;
      conv.warn.textContent = UNSAVED + again;
      return false;
    }
    discard();
    return true;
  }

  function hasUnsaved() {
    return failedSave || conv.box.value.trim() !== "" || conv.area.value !== (popNote.comment || "");
  }

  // Typing again or sending the reply makes the next Escape warn again.
  function hideWarn() {
    warned = false;
    if (conv) conv.warn.textContent = "";
  }
  // Registered always: a note can be open on a page that had none before.
  addEventListener("resize", () => { if (pop) place(pop, anchorRect(popNote)); });

  function anchorRect(note) {
    const a = anchors.get(note.id);
    if (a instanceof Range) return a.getBoundingClientRect();
    if (a) {
      const r = a.el.getBoundingClientRect();
      const x = r.left + a.point.x * r.width, y = r.top + a.point.y * r.height;
      return new DOMRect(x, y, 0, 0);
    }
    return new DOMRect(innerWidth / 2, 80, 0, 0);
  }

  function place(el, rect) {
    const { offsetWidth: width, offsetHeight: height } = el;
    el.style.left = Math.max(8, Math.min(rect.left, innerWidth - width - 8)) + "px";
    const below = rect.bottom + 8;
    if (below + height + 8 <= innerHeight) el.style.top = below + "px";
    else if (rect.top - 8 - height >= 8) el.style.top = rect.top - 8 - height + "px";
    else el.style.top = Math.max(8, innerHeight - height - 8) + "px";
  }

  // Enter sends or saves; Shift+Enter breaks the line. Not while an input method
  // is still composing a character.
  const submits = (e) => e.key === "Enter" && !e.shiftKey && !e.isComposing;

  function openNote(note, isNew = false) {
    if (!closePopSafely()) return;
    ensureHost();
    hideToolbar();
    pop = document.createElement("div");
    popNote = note;
    pop.className = "card pop";
    pop.setAttribute("role", "dialog");
    pop.setAttribute("aria-label", tr("Anotación", "Annotation"));
    const header = document.createElement("header");
    let name = null;
    if (note.target.kind === "text") {
      const exact = cut(clean(note.target.quote?.exact), 300);
      const quote = document.createElement("span");
      quote.className = "quote";
      quote.title = exact;
      const inner = document.createElement("span");
      inner.textContent = exact;
      quote.append("«", inner, "»");
      header.append(quote);
    } else {
      name = document.createElement("input");
      name.className = "name";
      name.maxLength = TITLE_MAX;
      name.value = titleOf(note);
      name.placeholder = tr("Título", "Title");
      name.setAttribute("aria-label", tr("Título de la anotación", "Annotation title"));
      name.title = tr("Título de la anotación. Haz clic para cambiarlo.", "Annotation title. Click to change it.");
      name.addEventListener("keydown", (e) => {
        // Stopped here: the handler of the whole popover would take this Enter as "save".
        if (e.key === "Enter" && !e.isComposing) { e.preventDefault(); e.stopPropagation(); area.focus(); }
      });
      header.append(name);
    }
    let color = note.color;
    const swatches = document.createElement("div");
    swatches.className = "swatches";
    swatches.setAttribute("role", "group");
    swatches.setAttribute("aria-label", tr("Color", "Color"));
    for (const [cname, value] of Object.entries(COLORS)) {
      const sw = document.createElement("button");
      sw.className = "swatch";
      sw.style.backgroundColor = value;
      sw.title = COLOR_NAMES[cname];
      sw.setAttribute("aria-label", "Color " + COLOR_NAMES[cname]);
      sw.setAttribute("aria-pressed", String(cname === color));
      sw.onclick = () => {
        color = cname;
        for (const s of swatches.children) s.setAttribute("aria-pressed", String(s === sw));
      };
      swatches.append(sw);
    }
    header.append(swatches);
    const area = document.createElement("textarea");
    area.placeholder = tr("Escribe una nota…", "Write a note…");
    area.value = note.comment || "";
    area.maxLength = MAX_COMMENT;
    area.setAttribute("aria-label", tr("Comentario", "Comment"));
    pop.append(header, area);
    const list = document.createElement("div");
    list.className = "replies";
    const form = document.createElement("div");
    form.className = "reply-form";
    const box = document.createElement("textarea");
    box.placeholder = tr("Escribe una respuesta…", "Write a reply…");
    box.setAttribute("aria-label", tr("Respuesta", "Reply"));
    box.maxLength = MAX_COMMENT;
    const err = document.createElement("p");
    err.className = "reply-err";
    err.setAttribute("role", "alert");
    const field = document.createElement("div");
    field.className = "reply-field";
    const answer = document.createElement("button");
    answer.className = "send";
    answer.type = "button";
    answer.title = tr("Enviar respuesta (Enter)", "Send reply (Enter)");
    answer.setAttribute("aria-label", tr("Enviar respuesta", "Send reply"));
    answer.innerHTML = SEND;
    answer.onclick = sendReply;
    field.append(box, answer);
    box.addEventListener("keydown", (e) => {
      if (submits(e)) { e.preventDefault(); sendReply(); }
    });
    box.addEventListener("input", hideWarn);
    box.addEventListener("input", markFilled);
    area.addEventListener("input", hideWarn);
    // After hideWarn, which clears the line this one writes. The field stops at the limit, so say so.
    const full = tr("Has llegado al máximo de 20 000 caracteres. No cabe más texto.", "You have reached the limit of 20,000 characters. No more text fits.");
    area.addEventListener("input", () => { if (area.value.length >= MAX_COMMENT) conv.warn.textContent = full; });
    box.addEventListener("input", () => { if (box.value.length >= MAX_COMMENT) err.textContent = full; else if (err.textContent === full) err.textContent = ""; });
    const warn = document.createElement("p");
    warn.className = "unsaved";
    warn.setAttribute("role", "alert");
    form.append(field, err);
    conv = { area, list, form, box, err, warn };
    renderThread(note);
    pop.append(list, form);
    const actions = document.createElement("div");
    actions.className = "actions";
    const del = document.createElement("button");
    del.className = "danger";
    del.textContent = tr("Eliminar", "Delete");
    del.onclick = () => { remove(note.id); closePop(); };
    const cancel = document.createElement("button");
    cancel.textContent = tr("Cancelar", "Cancel");
    cancel.onclick = closePop;
    const ok = document.createElement("button");
    ok.className = "primary";
    ok.textContent = tr("Guardar", "Save");
    ok.onclick = () => {
      const changed = { ...note, color, comment: area.value };
      if (name) changed.title = clip(clean(name.value)) || titleOf({ ...note, title: "" });
      save(changed);
      closePop();
    };
    if (!isNew || note.saved) actions.append(del);
    actions.append(cancel, ok);
    pop.append(warn, actions);
    // Enter in the reply box sends the reply; it must not also save the note.
    // On a focused button, Enter presses that button instead.
    pop.addEventListener("keydown", (e) => {
      if (submits(e) && e.target !== box && e.target.matches("textarea, input")) { e.preventDefault(); ok.click(); }
      if (e.key === "Escape") { e.stopPropagation(); closePopSafely(() => cancel.click(), ESCAPE_AGAIN); }
    });
    ui.append(pop);
    place(pop, anchorRect(note));
    list.scrollTop = list.scrollHeight;
    area.focus();
  }

  // ---------- conversation ----------

  // The person can answer a saved note once it has a comment or a reply.
  const canReply = (n) => n.saved && (n.comment?.trim() || n.replies?.length);

  function bubble(r) {
    const item = document.createElement("div");
    item.className = r.author === "agente" ? "reply" : "reply tu";
    const who = document.createElement("b");
    who.textContent = r.author === "agente" ? tr("Agente", "Agent") : tr("Tú", "You");
    item.append(who, document.createTextNode(r.text));
    return item;
  }

  // Redraws the messages but not the draft: keeps the scroll at the end if it was there.
  function renderThread(note) {
    const { list } = conv;
    const atEnd = list.scrollTop + list.clientHeight >= list.scrollHeight - 4;
    const was = list.scrollTop;
    list.replaceChildren(...(note.replies || []).map(bubble));
    conv.form.hidden = !canReply(note);
    list.scrollTop = atEnd ? list.scrollHeight : was;
  }

  // A class and not :has(:placeholder-shown): WebKitGTK does not restyle the
  // button when the text changes.
  function markFilled() {
    conv.box.parentElement.classList.toggle("filled", conv.box.value.trim() !== "");
  }

  function sendReply() {
    const text = conv.box.value.trim();
    if (!text || !popNote) return;
    const note = popNote;
    conv.box.value = "";
    markFilled();
    conv.err.textContent = "";
    hideWarn();
    note.replies = [...(note.replies || []), { author: "persona", text }];
    renderThread(note);
    conv.list.scrollTop = conv.list.scrollHeight;
    send({ type: "annot_reply", id: note.id, text });
  }

  // The app could not store the message: take it back out and say why.
  function replyFailed(id, text, error) {
    if (!popNote || popNote.id !== id) return;
    const replies = popNote.replies || [];
    for (let i = replies.length - 1; i >= 0; i--) {
      if (replies[i].author === "persona" && replies[i].text === text) { replies.splice(i, 1); break; }
    }
    if (!conv.box.value) conv.box.value = text;
    markFilled();
    conv.err.textContent = error;
    renderThread(popNote);
  }

  // An agent's reply arrived, or the notes were read again: show it in the open
  // note. The draft in the box is not touched.
  function refreshPopover() {
    if (!pop || !popNote) return;
    const fresh = notes.find((n) => n.id === popNote.id);
    if (!fresh) {
      if (popNote.saved) closePop();
      return;
    }
    popNote = fresh;
    renderThread(fresh);
  }

  // ---------- changes ----------

  const newId = () => crypto.randomUUID().slice(0, 8);

  // What a failed save or delete needs to undo: the note as it was, and whether
  // the person was editing it in the popover.
  const pending = new Map();
  const removed = new Map();

  function save(note) {
    pending.set(note.id, { before: notes.find((n) => n.id === note.id) || null, edited: popNote?.id === note.id });
    note.saved = true;
    const i = notes.findIndex((n) => n.id === note.id);
    if (i === -1) notes.push(note); else notes[i] = note;
    const { saved, ...clean } = note;
    send({ type: "annot_save", annotation: clean });
    paint();
  }

  function dropLocal(id) {
    notes = notes.filter((n) => n.id !== id);
    anchors.delete(id);
    paint();
  }

  function remove(id) {
    const note = notes.find((n) => n.id === id);
    if (note?.saved) removed.set(id, note);
    dropLocal(id);
    send({ type: "annot_delete", id });
  }

  const sentence = (s) => (/[.!?]$/.test(s) ? s : s + ".");

  // The app could not store the note. The text must not be lost: if the person was
  // editing it, the popover comes back with their text; otherwise the note goes
  // back to how it was on disk.
  function saveFailed(id, error) {
    const job = pending.get(id);
    pending.delete(id);
    const mine = notes.find((n) => n.id === id);
    if (!mine) return;
    const i = notes.indexOf(mine);
    if (job?.before) notes[i] = job.before; else mine.saved = false;
    if (job?.edited && !pop) {
      paint();
      openNote(mine, !job.before);
      failedSave = true;
      conv.warn.textContent = tr(`No se pudo guardar: ${sentence(error)} Tu texto sigue aquí; pulsa Guardar para intentarlo otra vez.`, `Could not save: ${sentence(error)} Your text is still here; press Save to try again.`);
      return;
    }
    if (!job?.before) dropLocal(id); else paint();
    showToast(tr(`No se pudo guardar la anotación. ${sentence(error)}`, `Could not save the annotation. ${sentence(error)}`));
  }

  function deleteFailed(id, error) {
    const note = removed.get(id);
    removed.delete(id);
    if (note && !notes.some((n) => n.id === id)) {
      notes.push(note);
      resolve();
    }
    showToast(tr(`No se pudo borrar la anotación. ${sentence(error)} Sigue en la página.`, `Could not delete the annotation. ${sentence(error)} It is still on the page.`));
  }

  // An error that does not belong to an open popover. It stays until dismissed.
  let toast = null;
  function closeToast() { toast?.remove(); toast = null; }
  function showToast(text) {
    ensureHost();
    closeToast();
    toast = document.createElement("div");
    toast.className = "card toast";
    toast.setAttribute("role", "alert");
    const message = document.createElement("span");
    message.textContent = text;
    const close = document.createElement("button");
    close.textContent = tr("Cerrar", "Close");
    close.onclick = closeToast;
    toast.append(message, close);
    ui.append(toast);
  }

  // ---------- selection toolbar ----------

  let toolbar = null;
  function hideToolbar() { toolbar?.remove(); toolbar = null; }

  function selectedRange() {
    const sel = getSelection();
    if (!sel || sel.isCollapsed || !sel.rangeCount) return null;
    const range = sel.getRangeAt(0);
    const start = range.startContainer.parentElement || range.startContainer;
    if (!document.body.contains(range.commonAncestorContainer)) return null;
    if (start.closest?.(SKIP + ",input,[contenteditable]")) return null;
    return range;
  }

  function showToolbar(range) {
    ensureHost();
    hideToolbar();
    toolbar = document.createElement("div");
    toolbar.className = "card toolbar";
    toolbar.setAttribute("role", "toolbar");
    toolbar.setAttribute("aria-label", tr("Anotar selección", "Annotate selection"));
    const make = (color, comment) => {
      // Before the note exists: a refused popover must not leave an unsaved note behind.
      if (comment && !closePopSafely()) return;
      const index = textIndex();
      const quote = quoteOf(index, range);
      if (!quote.exact.trim()) return hideToolbar();
      const note = { id: newId(), color, comment: "", replies: [], target: { kind: "text", quote } };
      anchors.set(note.id, rangeAt(index, quote.start, quote.start + quote.exact.length));
      getSelection().removeAllRanges();
      hideToolbar();
      if (comment) {
        notes.push(note);
        paint();
        openNote(note, true);
      } else {
        save(note);
      }
    };
    for (const [name, value] of Object.entries(COLORS)) {
      const b = document.createElement("button");
      b.className = "dot";
      b.title = tr("Resaltar en ", "Highlight in ") + COLOR_NAMES[name];
      b.setAttribute("aria-label", tr("Resaltar en ", "Highlight in ") + COLOR_NAMES[name]);
      const swatch = document.createElement("span");
      swatch.style.background = value;
      b.append(swatch);
      b.onmousedown = (e) => e.preventDefault();
      b.onclick = () => make(name, false);
      toolbar.append(b);
    }
    const c = document.createElement("button");
    c.className = "text";
    c.textContent = tr("Comentar", "Comment");
    c.onmousedown = (e) => e.preventDefault();
    c.onclick = () => make("amarillo", true);
    toolbar.append(c);
    ui.append(toolbar);
    const rects = range.getClientRects();
    const r = rects[rects.length - 1] || range.getBoundingClientRect();
    toolbar.style.left = Math.max(8, Math.min(r.right - 90, innerWidth - 190)) + "px";
    toolbar.style.top = (r.bottom + 8 + 40 < innerHeight ? r.bottom + 8 : Math.max(8, r.top - 44)) + "px";
  }

  addEventListener("mouseup", (e) => {
    if (e.composedPath().includes(host)) return;
    setTimeout(() => {
      const range = selectedRange();
      if (range) showToolbar(range);
    });
  });
  addEventListener("mousedown", (e) => {
    if (host && e.composedPath().includes(host)) return;
    hideToolbar();
  });
  addEventListener("scroll", hideToolbar, { passive: true });

  // A click on highlighted text opens its note, unless it was meant for a link or control.
  addEventListener("click", (e) => {
    if (picking || (host && e.composedPath().includes(host)) || !notes.length) return;
    if (!getSelection().isCollapsed || e.target.closest?.("a,button,input,select,textarea,label,summary")) return;
    const pos = document.caretPositionFromPoint(e.clientX, e.clientY);
    if (!pos) return;
    for (const note of notes) {
      const a = anchors.get(note.id);
      if (a instanceof Range && a.isPointInRange(pos.offsetNode, pos.offset)) return openNote(note);
    }
  });

  // ---------- element picking ----------

  let picking = false;
  let hover = null, banner = null, picked = null;

  function target(e) {
    const el = document.elementFromPoint(e.clientX, e.clientY);
    return el && el !== host && el !== document.documentElement && el !== document.body ? el : null;
  }

  function onPickMove(e) {
    picked = target(e);
    if (!picked) return (hover.style.display = "none");
    const r = picked.getBoundingClientRect();
    Object.assign(hover.style, { display: "", left: r.left + "px", top: r.top + "px", width: r.width + "px", height: r.height + "px" });
  }

  // Capture phase, so the page never sees the click that places the note.
  function onPickClick(e) {
    e.preventDefault();
    e.stopPropagation();
    const el = target(e);
    stopPicking();
    if (!el) return;
    const r = el.getBoundingClientRect();
    const point = { x: r.width ? (e.clientX - r.left) / r.width : 0.5, y: r.height ? (e.clientY - r.top) / r.height : 0.5 };
    const note = { id: newId(), color: "amarillo", title: defaultTitle(el), comment: "", replies: [], target: { kind: "element", element: describe(el), point } };
    notes.push(note);
    anchors.set(note.id, { el, point });
    paint();
    openNote(note, true);
  }

  // The listeners exist only while picking: otherwise every mouse move and click
  // on the page would call into this code just to return.
  function startPicking() {
    if (picking) return stopPicking();
    if (!closePopSafely()) return;
    ensureHost();
    picking = true;
    hover = document.createElement("div");
    hover.className = "hover";
    hover.style.display = "none";
    banner = document.createElement("div");
    banner.className = "card banner";
    banner.setAttribute("role", "status");
    banner.textContent = tr("Haz clic en un elemento o en un punto para anotarlo. Escape para salir.", "Click an element or a point to annotate it. Escape to exit.");
    ui.append(hover, banner);
    document.documentElement.style.cursor = "crosshair";
    addEventListener("mousemove", onPickMove, true);
    addEventListener("click", onPickClick, true);
  }

  function stopPicking() {
    picking = false;
    removeEventListener("mousemove", onPickMove, true);
    removeEventListener("click", onPickClick, true);
    hover?.remove(); banner?.remove();
    hover = banner = picked = null;
    document.documentElement.style.cursor = "";
  }

  addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (picking) stopPicking();
    else if (pop) closePopSafely(closePop, ESCAPE_AGAIN);
    else if (panel) closePanel();
    else closeToast();
    hideToolbar();
  });

  // ---------- list panel ----------

  let panel = null;
  function closePanel() { panel?.remove(); panel = null; }

  function togglePanel() {
    if (panel) return closePanel();
    ensureHost();
    panel = document.createElement("aside");
    panel.className = "card panel";
    panel.setAttribute("aria-label", tr("Anotaciones de la página", "Page annotations"));
    const header = document.createElement("header");
    const h = document.createElement("span");
    h.textContent = tr(`Anotaciones (${notes.length})`, `Annotations (${notes.length})`);
    const close = document.createElement("button");
    close.textContent = "×";
    close.setAttribute("aria-label", tr("Cerrar", "Close"));
    close.style.cssText = "font-size:18px;width:28px;height:28px";
    close.onclick = closePanel;
    header.append(h, close);
    panel.append(header);
    if (!notes.length) {
      const p = document.createElement("p");
      p.className = "empty";
      p.textContent = tr("Todavía no hay anotaciones. Selecciona texto o pulsa Ctrl+Shift+A para anotar un elemento.", "No annotations yet. Select text or press Ctrl+Shift+A to annotate an element.");
      panel.append(p);
    }
    const list = document.createElement("ul");
    const ordered = [...notes].sort((a, b) => (a.target.quote?.start ?? 1e12) - (b.target.quote?.start ?? 1e12));
    for (const note of ordered) {
      const li = document.createElement("li");
      const b = document.createElement("button");
      b.style.setProperty("--c", COLORS[note.color] || COLORS.amarillo);
      if (note.target.kind === "text") {
        const q = document.createElement("q");
        q.textContent = note.target.quote?.exact || "";
        b.append(q);
      } else {
        const what = document.createElement("strong");
        what.textContent = titleOf(note);
        b.append(what);
      }
      if (note.comment) b.append(document.createTextNode(note.comment));
      const meta = document.createElement("small");
      const n = note.replies?.length || 0;
      meta.textContent = [
        anchors.has(note.id) ? "" : note.target.kind === "text"
          ? tr("Ya no se encuentra en la página: el texto cambió o se borró.", "It is no longer on the page: the text changed or was deleted.")
          : tr("Ya no se encuentra en la página: el elemento cambió o se borró.", "It is no longer on the page: the element changed or was deleted."),
        n ? tr(`${n} respuesta${n > 1 ? "s" : ""}`, `${n} ${n > 1 ? "replies" : "reply"}`) : "",
      ].filter(Boolean).join(" · ");
      if (!anchors.has(note.id)) meta.className = "orphan";
      b.append(meta);
      b.onclick = () => {
        const a = anchors.get(note.id);
        const node = a instanceof Range ? a.startContainer.parentElement : a?.el;
        node?.scrollIntoView({ block: "center", behavior: "smooth" });
        setTimeout(() => openNote(note), node ? 350 : 0);
      };
      li.append(b);
      list.append(li);
    }
    panel.append(list);
    ui.append(panel);
  }

  // ---------- loading and keeping up with the page ----------

  async function load() {
    // Notes not saved yet belong to the open popover: a reload must keep them.
    const unsaved = notes.filter((n) => !n.saved);
    let saved = [];
    try {
      const response = await fetch(storeUrl, { cache: "no-store" });
      saved = response.ok ? (await response.json()).annotations || [] : [];
    } catch {
      saved = [];
    }
    for (const n of saved) n.saved = true;
    notes = [...saved, ...unsaved];
    resolve();
    if (notes.length) watch();
    if (panel) { closePanel(); togglePanel(); }
    refreshPopover();
  }

  // Pages built by scripts may move or replace text after load. Re-resolve when
  // an anchor broke, otherwise only move the markers. At most once per second.
  let watching = false;
  function watch() {
    if (watching) return;
    watching = true;
    let timer = 0;
    const later = () => {
      if (timer) return;
      timer = setTimeout(() => {
        timer = 0;
        const broken = notes.some((n) => {
          const a = anchors.get(n.id);
          return !a || (a instanceof Range ? a.collapsed || !a.startContainer.isConnected : !a.el.isConnected);
        });
        if (broken) resolve(); else placePins();
      }, 1000);
    };
    new MutationObserver((records) => {
      if (records.every((r) => host && (r.target === host || host.contains(r.target)))) return;
      later();
    }).observe(document.documentElement, { childList: true, subtree: true, characterData: true });
    new ResizeObserver(() => placePins()).observe(document.body);
    // Resize events can come several times a frame; one pass per frame is enough.
    // The body observer may still run in the same frame, which only reads rects now.
    let pinsQueued = false;
    addEventListener("resize", () => {
      if (pinsQueued) return;
      pinsQueued = true;
      requestAnimationFrame(() => { pinsQueued = false; placePins(); });
    });
  }

  // Ctrl+E (note.js) marks the page while the source is on screen. Own observer,
  // without the one-second delay of the mutation one: the markers must not sit on the editor.
  new MutationObserver(() => {
    const editing = !!document.documentElement.dataset.visorEditing;
    if (host) host.style.display = editing ? "none" : "";
    if (!editing) return placePins();
    // The popover stays, hidden with the layer: it may hold text the person has not saved.
    // note.js asks leaveOk() before opening the editor, because leaving it reloads the page.
    if (picking) stopPicking();
    closePanel();
    closeToast();
    hideToolbar();
  }).observe(document.documentElement, { attributes: true, attributeFilter: ["data-visor-editing"] });

  window.__visorAnnot = Object.freeze({
    pickElement: startPicking,
    togglePanel,
    reload: load,
    replyFailed,
    // Ctrl+E asks before it reloads the page away from an open popover with unsaved text.
    leaveOk: () => closePopSafely(() => {}, tr("Pulsa Ctrl+E otra vez para abrir el editor y descartarlo.", "Press Ctrl+E again to open the editor and discard it.")),
    saveFailed,
    deleteFailed,
  });

  if (document.readyState === "complete") load();
  else addEventListener("load", load, { once: true });
})();
