// Annotation layer for vault pages: text highlights and notes pinned to
// elements, drawn over the page without touching its DOM. Highlights use the
// CSS Custom Highlight API; markers, toolbars and dialogs live in a shadow root
// attached to <html>, outside <body>, so the page's own text never includes them.
(() => {
  const BASE = "__VISOR_BASE__";
  if (window.top !== window || window.__visorAnnot || !location.href.startsWith(BASE)) return;

  const page = decodeURIComponent(location.href.slice(BASE.length).split(/[?#]/)[0]);
  const storeUrl = BASE + ".visor/anotaciones/" + page.split("/").map(encodeURIComponent).join("/") + ".json";
  const send = (m) => window.ipc.postMessage(JSON.stringify(m));
  const COLORS = { amarillo: "#f2c94c", verde: "#6fcf97", azul: "#56a0f5", rosa: "#f28bb0" };
  const SKIP = "script,style,noscript,textarea,template,[data-visor-skip]";
  const CONTEXT = 32;

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
      exact,
      prefix: index.text.slice(Math.max(0, start - CONTEXT), start),
      suffix: index.text.slice(end, end + CONTEXT),
      start,
    };
  }

  // ---------- elements ----------

  const snippet = (el) => (el.textContent || "").replace(/\s+/g, " ").trim().slice(0, 80);

  function describe(el) {
    const path = [];
    for (let n = el; n && n !== document.body && n.nodeType === 1; n = n.parentElement) {
      if (n.id && document.querySelectorAll("#" + CSS.escape(n.id)).length === 1) {
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
      id: el.id || null,
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
    return null;
  }

  // ---------- titles for element notes ----------

  const TITLE_MAX = 120;
  const clip = (s) => (s.length > TITLE_MAX ? s.slice(0, TITLE_MAX - 1).trimEnd() + "…" : s);
  const clean = (s) => (s || "").replace(/\s+/g, " ").trim();

  // What a person would call the element, never its tag name.
  function kindOf(tag, el) {
    if (el?.closest?.("table")) return "Tabla";
    if (el?.closest?.("pre")) return "Código";
    if (el?.closest?.(".katex,math,mjx-container")) return "Fórmula";
    if (el instanceof SVGElement) return outerSvg(el).getAttribute("role") === "img" ? "Imagen" : "Diagrama";
    const kinds = {
      img: "Imagen", picture: "Imagen", figure: "Figura", canvas: "Diagrama", svg: "Diagrama",
      video: "Vídeo", audio: "Audio", iframe: "Contenido incrustado", embed: "Contenido incrustado", object: "Contenido incrustado",
      table: "Tabla", caption: "Tabla", thead: "Tabla", tbody: "Tabla", tr: "Tabla", td: "Tabla", th: "Tabla",
      pre: "Código", code: "Código", math: "Fórmula",
      ul: "Lista", ol: "Lista", li: "Lista", dl: "Lista",
      p: "Párrafo", blockquote: "Cita", a: "Enlace",
      button: "Control", input: "Control", select: "Control", textarea: "Control", label: "Control",
      h1: "Encabezado", h2: "Encabezado", h3: "Encabezado", h4: "Encabezado", h5: "Encabezado", h6: "Encabezado",
    };
    return kinds[tag] || "Elemento";
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
    if (kind === "Encabezado") return clip(`Encabezado “${clean(el.textContent)}”`);
    const section = sectionOf(el);
    return clip(section ? `${kind} en “${section}”` : kind);
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
    .ui { font: 13px/1.45 system-ui, sans-serif; color: var(--visor-fg, #222);
      --soft: color-mix(in srgb, var(--visor-fg, #222) 80%, var(--visor-bg, #fff));
      --danger: color-mix(in srgb, var(--visor-red, #c0392b) 60%, var(--visor-fg, #222));
      --strong: color-mix(in srgb, var(--visor-accent, #1a6cc4) 65%, var(--visor-fg, #222)); }
    .card { background: color-mix(in srgb, var(--visor-fg, #222) 4%, var(--visor-bg, #fff)); color: var(--visor-fg, #222);
      border: 1px solid color-mix(in srgb, var(--visor-fg, #222) 28%, var(--visor-bg, #fff));
      border-radius: 10px; z-index: 1; box-shadow: 0 2px 6px rgb(0 0 0 / .18), 0 12px 32px rgb(0 0 0 / .28); }
    button { font: inherit; color: inherit; background: none; border: 0; cursor: pointer; border-radius: 6px; }
    button:focus-visible, textarea:focus-visible, input:focus-visible { outline: 2px solid var(--visor-accent, #1a6cc4); outline-offset: 1px; }
    .pin { position: absolute; width: 22px; height: 22px; margin: -11px 0 0 -11px; border-radius: 50%;
      display: grid; place-items: center; pointer-events: auto; cursor: pointer;
      border: 2px solid var(--visor-bg, #fff); box-shadow: 0 1px 4px rgb(0 0 0 / .35); }
    .pin.text { width: 14px; height: 14px; margin: -7px 0 0 -3px; }
    .pin svg { width: 12px; height: 12px; }
    .toolbar { position: fixed; display: flex; gap: 2px; padding: 4px; pointer-events: auto; }
    .toolbar .dot { width: 26px; height: 26px; display: grid; place-items: center; }
    .toolbar .dot span { width: 16px; height: 16px; border-radius: 50%; }
    .toolbar .dot:hover, .toolbar .text:hover { background: color-mix(in srgb, var(--visor-fg, #222) 10%, transparent); }
    .toolbar .text { padding: 0 10px; }
    /* minmax(0, 1fr): without it the column grows to the unwrapped quote and the content spills out of the card. */
    .pop { position: fixed; width: min(340px, calc(100vw - 16px)); max-height: calc(100vh - 16px); overflow: auto;
      padding: 12px; pointer-events: auto; display: grid; grid-template-columns: minmax(0, 1fr); gap: 8px; }
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
    .replies { display: grid; gap: 6px; max-height: 180px; overflow: auto; }
    .reply { padding: 6px 8px; border-radius: 6px; white-space: pre-wrap;
      background: color-mix(in srgb, var(--visor-accent, #1a6cc4) 10%, var(--visor-bg, #fff)); }
    .reply b { display: block; font-size: 11px; color: var(--soft); font-weight: 600; }
    .actions { display: flex; flex-wrap: wrap; gap: 6px; justify-content: flex-end; }
    .actions button { padding: 5px 10px; border: 1px solid color-mix(in srgb, var(--visor-fg, #222) 30%, var(--visor-bg, #fff));
      background: color-mix(in srgb, var(--visor-fg, #222) 4%, var(--visor-bg, #fff)); }
    .actions button:hover { background: color-mix(in srgb, var(--visor-fg, #222) 10%, var(--visor-bg, #fff)); }
    .actions .primary, .actions .primary:hover { background: var(--strong); border-color: var(--strong);
      color: var(--visor-bg, #fff); font-weight: 600; }
    .actions .primary:hover { filter: brightness(1.1); }
    .actions .danger { color: var(--danger); margin-right: auto;
      border-color: color-mix(in srgb, var(--danger) 50%, var(--visor-bg, #fff)); }
    .hover { position: fixed; pointer-events: none; border: 2px solid var(--visor-accent, #1a6cc4); border-radius: 4px;
      background: color-mix(in srgb, var(--visor-accent, #1a6cc4) 12%, transparent); }
    .banner { position: fixed; top: 12px; left: 50%; translate: -50% 0; padding: 8px 14px; pointer-events: auto; }
    .panel { position: fixed; top: 12px; right: 12px; bottom: 12px; width: min(330px, calc(100vw - 24px)); padding: 12px; pointer-events: auto;
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
  `;
  const ICON = `<svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2 2h8v6H5l-3 2z" fill="currentColor"/></svg>`;

  function ensureHost() {
    if (host) return;
    host = document.createElement("visor-capa");
    host.style.cssText = "position:absolute;top:0;left:0;width:0;height:0;z-index:2147483647;pointer-events:none";
    const shadow = host.attachShadow({ mode: "open" });
    const style = document.createElement("style");
    style.textContent = CSS_TEXT;
    ui = document.createElement("div");
    ui.className = "ui";
    shadow.append(style, ui);
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

  function placePins() {
    const needPins = notes.some((n) => anchors.has(n.id) && (n.target.kind === "element" || n.comment || n.replies?.length));
    if (!needPins && !host) return;
    ensureHost();
    for (const pin of ui.querySelectorAll(".pin")) pin.remove();
    host.style.display = document.documentElement.dataset.visorEditing ? "none" : "";
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
      const pin = document.createElement("button");
      pin.className = cls;
      pin.style.cssText = `left:${x + scrollX}px;top:${y + scrollY}px;background:${COLORS[note.color] || COLORS.amarillo};color:#222`;
      pin.title = note.comment || note.title || "Anotación";
      pin.setAttribute("aria-label", "Abrir anotación");
      if (cls === "pin") pin.innerHTML = ICON;
      pin.onclick = (e) => { e.stopPropagation(); openNote(note); };
      ui.append(pin);
    }
  }

  // ---------- popover ----------

  let pop = null, popNote = null;
  function closePop() { pop?.remove(); pop = popNote = null; }
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

  function openNote(note, isNew = false) {
    ensureHost();
    closePop();
    hideToolbar();
    pop = document.createElement("div");
    popNote = note;
    pop.className = "card pop";
    pop.setAttribute("role", "dialog");
    pop.setAttribute("aria-label", "Anotación");
    const header = document.createElement("header");
    let name = null;
    if (note.target.kind === "text") {
      const exact = clean(note.target.quote?.exact).slice(0, 300);
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
      name.placeholder = "Título";
      name.setAttribute("aria-label", "Título de la anotación");
      name.title = "Título de la anotación. Haz clic para cambiarlo.";
      name.addEventListener("keydown", (e) => {
        if (e.key === "Enter") { e.preventDefault(); area.focus(); }
      });
      header.append(name);
    }
    let color = note.color;
    const swatches = document.createElement("div");
    swatches.className = "swatches";
    swatches.setAttribute("role", "group");
    swatches.setAttribute("aria-label", "Color");
    for (const [cname, value] of Object.entries(COLORS)) {
      const sw = document.createElement("button");
      sw.className = "swatch";
      sw.style.backgroundColor = value;
      sw.title = cname;
      sw.setAttribute("aria-label", "Color " + cname);
      sw.setAttribute("aria-pressed", String(cname === color));
      sw.onclick = () => {
        color = cname;
        for (const s of swatches.children) s.setAttribute("aria-pressed", String(s === sw));
      };
      swatches.append(sw);
    }
    header.append(swatches);
    const area = document.createElement("textarea");
    area.placeholder = "Escribe una nota…";
    area.value = note.comment || "";
    pop.append(header, area);
    if (note.replies?.length) {
      const list = document.createElement("div");
      list.className = "replies";
      for (const r of note.replies) {
        const item = document.createElement("div");
        item.className = "reply";
        const who = document.createElement("b");
        who.textContent = r.author === "agente" ? "Agente" : "Tú";
        item.append(who, document.createTextNode(r.text));
        list.append(item);
      }
      pop.append(list);
    }
    const actions = document.createElement("div");
    actions.className = "actions";
    const del = document.createElement("button");
    del.className = "danger";
    del.textContent = "Eliminar";
    del.onclick = () => { remove(note.id); closePop(); };
    const cancel = document.createElement("button");
    cancel.textContent = "Cancelar";
    cancel.onclick = () => { if (isNew && !note.saved) dropLocal(note.id); closePop(); };
    const ok = document.createElement("button");
    ok.className = "primary";
    ok.textContent = "Guardar";
    ok.onclick = () => {
      const changed = { ...note, color, comment: area.value };
      if (name) changed.title = clip(clean(name.value)) || titleOf({ ...note, title: "" });
      save(changed);
      closePop();
    };
    if (!isNew || note.saved) actions.append(del);
    actions.append(cancel, ok);
    pop.append(actions);
    pop.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) ok.click();
      if (e.key === "Escape") { e.stopPropagation(); cancel.click(); }
    });
    ui.append(pop);
    place(pop, anchorRect(note));
    area.focus();
  }

  // ---------- changes ----------

  const newId = () => crypto.randomUUID().slice(0, 8);

  function save(note) {
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
    dropLocal(id);
    send({ type: "annot_delete", id });
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
    toolbar.setAttribute("aria-label", "Anotar selección");
    const make = (color, comment) => {
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
      b.title = "Resaltar en " + name;
      b.setAttribute("aria-label", "Resaltar en " + name);
      b.innerHTML = `<span style="background:${value}"></span>`;
      b.onmousedown = (e) => e.preventDefault();
      b.onclick = () => make(name, false);
      toolbar.append(b);
    }
    const c = document.createElement("button");
    c.className = "text";
    c.textContent = "Comentar";
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

  function startPicking() {
    if (picking) return stopPicking();
    ensureHost();
    closePop();
    picking = true;
    hover = document.createElement("div");
    hover.className = "hover";
    hover.style.display = "none";
    banner = document.createElement("div");
    banner.className = "card banner";
    banner.textContent = "Haz clic en un elemento o en un punto para anotarlo. Escape para salir.";
    ui.append(hover, banner);
    document.documentElement.style.cursor = "crosshair";
  }

  function stopPicking() {
    picking = false;
    hover?.remove(); banner?.remove();
    hover = banner = picked = null;
    document.documentElement.style.cursor = "";
  }

  function target(e) {
    const el = document.elementFromPoint(e.clientX, e.clientY);
    return el && el !== host && el !== document.documentElement && el !== document.body ? el : null;
  }

  addEventListener("mousemove", (e) => {
    if (!picking) return;
    picked = target(e);
    if (!picked) return (hover.style.display = "none");
    const r = picked.getBoundingClientRect();
    Object.assign(hover.style, { display: "", left: r.left + "px", top: r.top + "px", width: r.width + "px", height: r.height + "px" });
  }, true);

  // Capture phase, so the page never sees the click that places the note.
  addEventListener("click", (e) => {
    if (!picking) return;
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
  }, true);

  addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (picking) stopPicking();
    else if (pop) closePop();
    else if (panel) closePanel();
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
    panel.setAttribute("aria-label", "Anotaciones de la página");
    const header = document.createElement("header");
    const h = document.createElement("span");
    h.textContent = `Anotaciones (${notes.length})`;
    const close = document.createElement("button");
    close.textContent = "×";
    close.setAttribute("aria-label", "Cerrar");
    close.style.cssText = "font-size:18px;width:28px;height:28px";
    close.onclick = closePanel;
    header.append(h, close);
    panel.append(header);
    if (!notes.length) {
      const p = document.createElement("p");
      p.className = "empty";
      p.textContent = "Todavía no hay anotaciones. Selecciona texto o pulsa Ctrl+Shift+A para anotar un elemento.";
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
        anchors.has(note.id) ? "" : "Huérfana: no se encuentra en la página",
        n ? `${n} respuesta${n > 1 ? "s" : ""}` : "",
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
    try {
      const response = await fetch(storeUrl, { cache: "no-store" });
      notes = response.ok ? (await response.json()).annotations || [] : [];
    } catch {
      notes = [];
    }
    for (const n of notes) n.saved = true;
    resolve();
    if (notes.length) watch();
    if (panel) { closePanel(); togglePanel(); }
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
    }).observe(document.documentElement, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["data-visor-editing"] });
    new ResizeObserver(() => placePins()).observe(document.body);
    addEventListener("resize", placePins);
  }

  window.__visorAnnot = Object.freeze({
    pickElement: startPicking,
    togglePanel,
    reload: load,
  });

  if (document.readyState === "complete") load();
  else addEventListener("load", load, { once: true });
})();
