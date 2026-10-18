(() => {
  const tr = (es, en) => ("__VISOR_LANG__" === "en" ? en : es);
  const config = window.__visorNoteConfig;
  const view = document.getElementById("note");
  const edit = document.getElementById("edit");
  const editor = document.getElementById("editor");
  const status = document.getElementById("status");
  const send = (m) => window.__visorPost(m);
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
    if (config.mermaid && window.pele) {
      // The id prefix follows each diagram's place in the page, so it is the same on every load.
      [...view.querySelectorAll("pre.mermaid")].forEach((el, i) => {
        const text = el.textContent;
        el.textContent = "";
        diagram(el, text, { idPrefix: "d" + i + "-" });
      });
    }
    const saved = sessionStorage.getItem(scrollKey);
    if (saved !== null) {
      sessionStorage.removeItem(scrollKey);
      place(JSON.parse(saved));
    }
  }

  // Pele draws a diagram into el at the width of the column. Zoom controls appear
  // only when the diagram does not fit; their labels follow the page's language.
  // Pele's error text is English and technical, so it goes to the console and the
  // page gets a plain sentence.
  function diagram(el, text, options = {}) {
    return pele.mount(el, text, {
      ...options,
      zoom: { labels: { zoomIn: tr("Acercar", "Zoom in"), zoomOut: tr("Alejar", "Zoom out"), reset: tr("Restablecer el zoom", "Reset zoom") } },
      onError: (e) => {
        console.error("Diagrama: " + e.message);
        el.classList.add("md-error");
        el.textContent = diagramError(e);
      },
    });
  }

  // Pele gives a line only when it knows where the problem is (0 otherwise).
  function diagramError(e) {
    if (e.line > 0) return tr(`No se pudo dibujar el diagrama: hay un error en la línea ${e.line}, columna ${e.column}.`, `The diagram could not be drawn: there is an error on line ${e.line}, column ${e.column}.`);
    if (e.code === "unsupported-diagram") return tr("No se pudo dibujar el diagrama: no se reconoce el tipo de diagrama.", "The diagram could not be drawn: the diagram type is not recognized.");
    return tr("No se pudo dibujar el diagrama.", "The diagram could not be drawn.");
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
    hold();
  }

  // Images, maths and diagrams change heights after a placement, until the
  // person scrolls by hand. Installed by each place(), removed by the first gesture.
  const gestures = ["wheel", "keydown", "mousedown", "touchstart"];
  let guard = null;
  function hold() {
    if (guard) return;
    guard = new ResizeObserver(() => placed && place(placed));
    guard.observe(view);
    for (const type of gestures) addEventListener(type, release, { passive: true });
  }
  function release() {
    placed = null;
    guard.disconnect();
    guard = null;
    for (const type of gestures) removeEventListener(type, release, { passive: true });
  }

  // Reloads the rendered view, keeping the reading position.
  function reload(target = viewAnchor() || { y: scrollY }) {
    sessionStorage.setItem(scrollKey, JSON.stringify(target));
    location.replace(location.pathname);
  }

  // CodeMirror and the live preview (editor.js) load on the first Ctrl+E only.
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
        script.onerror = () => reject(new Error(tr("No se pudo cargar ", "Could not load ") + file));
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
      console.error(tr("No se pudo abrir el editor: ", "Could not open the editor: ") + e.message);
      notify(tr("No se pudo abrir el editor. Prueba otra vez con Ctrl+E.", "Could not open the editor. Try again with Ctrl+E."));
      return;
    }
    editing = true;
    dirty = false;
    typed = false;
    sent.length = 0;
    edit.hidden = false;
    // Hides the annotation layer while the source is on screen.
    document.documentElement.dataset.visorEditing = "1";
    say(tr("Editando. Se guarda solo. Ctrl+E para volver a la vista.", "Editing. Saves automatically. Ctrl+E to return to the view."));
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
    say(tr("Sin guardar", "Unsaved"));
    clearTimeout(timer);
    timer = setTimeout(save, 800);
  }

  function save() {
    clearTimeout(timer);
    if (!dirty) return;
    say(tr("Guardando…", "Saving…"));
    const text = ed.text();
    sent.push(text);
    send({ type: "save_note", text });
  }

  // Texts sent and not yet confirmed, oldest first; visor answers in order. Typing
  // while a save is in flight must not count as saved when its answer arrives.
  const sent = [];

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

  // A message about something done outside the editor, which has its own status line.
  // It stays until dismissed: the person may not be looking when it appears.
  function notify(text) {
    document.getElementById("aviso")?.remove();
    const box = document.createElement("div");
    box.id = "aviso";
    box.setAttribute("role", "alert");
    box.dataset.visorSkip = "";
    const message = document.createElement("span");
    message.textContent = text;
    const close = document.createElement("button");
    close.textContent = tr("Cerrar", "Close");
    close.onclick = () => box.remove();
    box.append(message, close);
    document.body.append(box);
  }

  window.__visorNote = {
    get editing() { return editing; },
    // Text the user typed that is not saved yet, or a save still on its way.
    get unsaved() { return dirty || sent.length > 0; },
    // Ctrl+F, sent by Rust: the editor has its own search; reading uses visor's bar.
    find() {
      if (!editing || !ed) return false;
      ed.find();
      return true;
    },
    createFailed(error) {
      notify(tr(`No se pudo crear la nota. ${/[.!?]$/.test(error) ? error : error + "."}`, `Could not create the note. ${/[.!?]$/.test(error) ? error : error + "."}`));
    },
    diagram,
    // Before visor hides the tab, which it may unload later: saves pending
    // text and returns the reading position for place().
    hide() {
      if (dirty) save();
      return viewAnchor() || { y: scrollY };
    },
    place(target) { place(target); },
    toggle() {
      if (editing) close();
      else if (!window.__visorAnnot || window.__visorAnnot.leaveOk()) open();
    },
    saved(ok, error) {
      const confirmed = sent.shift();
      if (!ok) return say(tr("No se pudo guardar: ", "Could not save: ") + error, true);
      // The text may have changed since this save was sent; those changes are still pending.
      if (confirmed !== undefined && ed && ed.text() !== confirmed) {
        say(sent.length ? tr("Guardando…", "Saving…") : tr("Sin guardar", "Unsaved"));
        return;
      }
      dirty = false;
      if (leaving) return reload(back);
      say(tr("Guardado", "Saved"));
    },
    // The file no longer matches the page: show what it really says.
    toggled(error) {
      if (!error) return;
      console.error(tr("No se pudo marcar la tarea: ", "Could not toggle the task: ") + error);
      reload();
    },
    // The file changed on disk, written by someone else.
    changed() {
      if (editing) say(tr("El archivo cambió fuera de visor. Si sigues escribiendo, lo sobrescribes.", "The file changed outside visor. If you keep writing, you will overwrite it."), true);
      else reload();
    },
  };

  if (document.readyState === "loading") addEventListener("DOMContentLoaded", render);
  else render();
  // New notes arrive with #editar: straight to writing, after the title.
  if (location.hash === "#editar") open(true);
})();
