(() => {
  const config = window.__visorNoteConfig;
  const view = document.getElementById("note");
  const edit = document.getElementById("edit");
  const editor = document.getElementById("editor");
  const status = document.getElementById("status");
  const send = (m) => window.ipc.postMessage(JSON.stringify(m));
  const scrollKey = "visor-scroll:" + location.pathname;
  let editing = false;
  let dirty = false;
  let typed = false;
  let opened = null;
  let leaving = false;
  let timer = 0;

  const say = (text, error = false) => {
    status.textContent = text;
    status.classList.toggle("error", error);
  };

  function render() {
    if (config.math && window.katex) {
      for (const el of view.querySelectorAll(".math")) {
        katex.render(el.textContent, el, { displayMode: el.classList.contains("math-display"), throwOnError: false });
      }
    }
    if (config.code && window.hljs) {
      for (const el of view.querySelectorAll("pre code[class*='language-']")) hljs.highlightElement(el);
    }
    if (config.mermaid && window.mermaid) {
      const mode = getComputedStyle(document.documentElement).getPropertyValue("--visor-mode").trim();
      const dark = mode ? mode === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
      mermaid.initialize({ startOnLoad: false, theme: dark ? "dark" : "default" });
      mermaid.run({ querySelector: "pre.mermaid" }).catch((e) => console.error("Mermaid: " + e.message));
    }
    const saved = sessionStorage.getItem(scrollKey);
    if (saved !== null) {
      sessionStorage.removeItem(scrollKey);
      place(JSON.parse(saved));
    }
  }

  // Blocks carry their source lines (data-line, data-end) from markdown.rs.
  const blocks = () => [...view.querySelectorAll("[data-line]")].filter((el) => el.getClientRects().length);

  // Source lines of a block, as [first, one past the last].
  const span = (el) => [+el.dataset.line, +(el.dataset.end || el.dataset.line) + 1];

  // The source line at the top of the rendered view and its height on screen.
  function viewAnchor() {
    const list = blocks();
    let found = -1;
    for (let i = 0; i < list.length; i++) {
      const r = list[i].getBoundingClientRect();
      if (r.top <= 0 && r.bottom > 0) found = i; // the deepest block across the top edge
      else if (r.top > 0) {
        if (found < 0) found = i;
        break;
      }
    }
    if (found < 0) return null;
    const r = list[found].getBoundingClientRect();
    const [start, end] = span(list[found]);
    if (r.top >= 0) return { line: start, y: r.top };
    // A long block (code, a paragraph that wraps) is cut by the edge: interpolate.
    const line = start + Math.floor((-r.top / r.height) * (end - start));
    return { line, y: r.top + ((line - start) / (end - start)) * r.height };
  }

  // Scrolls the rendered view so that source `line` sits `y` pixels from the top.
  let placed = null;
  function place(target) {
    if (target.line === undefined) return scrollTo(0, target.y);
    const list = blocks();
    let i = -1;
    while (i + 1 < list.length && +list[i + 1].dataset.line <= target.line) i++;
    if (i < 0) return;
    const r = list[i].getBoundingClientRect();
    const [start, end] = span(list[i]);
    const within = Math.min(1, (target.line - start) / (end - start));
    scrollTo(0, scrollY + r.top + within * r.height - target.y);
    placed = target;
  }
  // Images, maths and diagrams change heights after the first placement, until
  // the person scrolls by hand.
  new ResizeObserver(() => placed && place(placed)).observe(view);
  for (const type of ["wheel", "keydown", "mousedown", "touchstart"]) {
    addEventListener(type, () => { placed = null; }, { passive: true });
  }

  // Reloads the rendered view, keeping the reading position.
  function reload(target = viewAnchor() || { y: scrollY }) {
    sessionStorage.setItem(scrollKey, JSON.stringify(target));
    location.replace(location.pathname);
  }

  // CodeMirror and the live preview (src/editor.js) load on the first Ctrl+E only.
  let loading = null;
  function loadEditor() {
    return (loading ||= new Promise((resolve, reject) => {
      const css = document.createElement("link");
      css.rel = "stylesheet";
      css.href = config.assets + "visor/editor.css";
      document.head.append(css);
      const files = ["codemirror/codemirror.min.js", "visor/editor.js"];
      let left = files.length;
      for (const file of files) {
        const script = document.createElement("script");
        script.src = config.assets + file;
        script.async = false; // in order: editor.js needs CodeMirror
        script.onload = () => --left || resolve();
        script.onerror = () => reject(new Error("No se pudo cargar " + file));
        document.head.append(script);
      }
    }));
  }

  let ed = null;
  let opening = false;
  async function open(atEnd = false) {
    if (opening) return;
    opening = true;
    const anchor = atEnd ? null : viewAnchor();
    opened = null;
    let text;
    try {
      [text] = await Promise.all([
        fetch(location.pathname + "?raw", { cache: "no-store" }).then((r) => r.text()),
        loadEditor(),
      ]);
    } catch (e) {
      opening = false;
      loading = null;
      console.error("No se pudo abrir el editor: " + e.message);
      return;
    }
    editing = true;
    dirty = false;
    typed = false;
    edit.hidden = false;
    // Hides the annotation layer while the source is on screen.
    document.documentElement.dataset.visorEditing = "1";
    say("Editando. Se guarda solo. Ctrl+E para volver a la vista.");
    editor.replaceChildren();
    ed = window.__visorEditor.create(editor, text, { input: onInput, save });
    ed.focus();
    if (anchor) await ed.place(anchor);
    else if (atEnd) ed.atEnd();
    opened = { anchor, scrollTop: ed.scrollTop };
    opening = false;
  }

  function onInput() {
    dirty = typed = true;
    say("Sin guardar");
    clearTimeout(timer);
    timer = setTimeout(save, 800);
  }

  function save() {
    clearTimeout(timer);
    if (!dirty) return;
    say("Guardando…");
    send({ type: "save_note", text: ed.text() });
  }

  let back = null;
  function close() {
    if (!ed) return;
    // Untouched, it returns exactly where it was: near the end of the note
    // the editor cannot always scroll as far as the view did.
    const same = opened && opened.anchor && !typed && ed.scrollTop === opened.scrollTop;
    back = same ? opened.anchor : ed.anchor(typed);
    if (dirty) {
      leaving = true;
      save();
    } else {
      reload(back);
    }
  }

  addEventListener("beforeunload", () => { if (dirty) save(); });

  // Task checkboxes save on click; Rust checks that a task starts at that offset.
  view.addEventListener("change", (e) => {
    const box = e.target;
    if (!box.matches("input[data-task]")) return;
    send({ type: "toggle_task", offset: +box.dataset.task, checked: box.checked });
  });

  // [[links]] to notes that do not exist yet create them, as in Obsidian.
  document.addEventListener("click", (e) => {
    const link = e.target.closest('a[href^="#visor-new:"]');
    if (!link) return;
    e.preventDefault();
    const name = decodeURIComponent(link.getAttribute("href").slice("#visor-new:".length));
    send({ type: "create_note", dir: config.dir, name });
  });

  window.__visorNote = {
    get editing() { return editing; },
    // Before visor hides the tab, which it may unload later: saves pending
    // text and returns the reading position for place().
    hide() {
      if (dirty) save();
      return viewAnchor() || { y: scrollY };
    },
    place(target) { place(target); },
    toggle() { editing ? close() : open(); },
    saved(ok, error) {
      if (!ok) return say("No se pudo guardar: " + error, true);
      dirty = false;
      if (leaving) return reload(back);
      say("Guardado");
    },
    // The file no longer matches the page: show what it really says.
    toggled(error) {
      if (!error) return;
      console.error("No se pudo marcar la tarea: " + error);
      reload();
    },
    // The file changed on disk, written by someone else.
    changed() {
      if (editing) say("El archivo cambió fuera de visor. Si sigues escribiendo, lo sobrescribes.", true);
      else reload();
    },
  };

  if (document.readyState === "loading") addEventListener("DOMContentLoaded", render);
  else render();
  // New notes arrive with #editar: straight to writing, after the title.
  if (location.hash === "#editar") open(true);
})();
