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
    const y = sessionStorage.getItem(scrollKey);
    if (y !== null) {
      sessionStorage.removeItem(scrollKey);
      scrollTo(0, +y);
    }
  }

  // Reloads the rendered view, keeping the reading position.
  function reload() {
    sessionStorage.setItem(scrollKey, String(scrollY));
    location.replace(location.pathname);
  }

  async function open(atEnd = false) {
    const response = await fetch(location.pathname + "?raw", { cache: "no-store" });
    editor.value = await response.text();
    editing = true;
    dirty = false;
    edit.hidden = false;
    // Hides the annotation layer while the source is on screen.
    document.documentElement.dataset.visorEditing = "1";
    say("Editando. Se guarda solo. Ctrl+E para volver a la vista.");
    editor.focus();
    const caret = atEnd ? editor.value.length : 0;
    editor.setSelectionRange(caret, caret);
  }

  function save() {
    clearTimeout(timer);
    if (!dirty) return;
    say("Guardando…");
    send({ type: "save_note", text: editor.value });
  }

  function close() {
    if (dirty) {
      leaving = true;
      save();
    } else {
      reload();
    }
  }

  editor.addEventListener("input", () => {
    dirty = true;
    say("Sin guardar");
    clearTimeout(timer);
    timer = setTimeout(save, 800);
  });
  editor.addEventListener("keydown", (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    } else if (e.key === "Tab" && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      editor.setRangeText("\t", editor.selectionStart, editor.selectionEnd, "end");
      editor.dispatchEvent(new Event("input"));
    }
  });
  addEventListener("beforeunload", () => { if (dirty) save(); });

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
    toggle() { editing ? close() : open(); },
    saved(ok, error) {
      if (!ok) return say("No se pudo guardar: " + error, true);
      dirty = false;
      if (leaving) return reload();
      say("Guardado");
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
