// The page of a drawing: Excalidraw, read-only until Ctrl+E. What the person
// draws is sent to Rust, which checks the path and writes the file (save_drawing
// in src/ui/app.rs). drawing-format.js, loaded before this, reads and writes the
// Obsidian form.
(() => {
  const config = window.__visorDrawingConfig;
  const fmt = window.__visorDrawingFormat;
  const { React, createRoot, Excalidraw, serializeAsJSON, LZString } = window.Excalidraw;
  const md = config.kind === "md";
  const status = document.getElementById("status");
  const mount = document.getElementById("dibujo");
  const send = (m) => window.__visorPost(m);
  // Same pause as the notes' editor: a save follows the last change by this many ms.
  const DELAY = 800;

  let root = null;
  let initial = null;
  let editing = false;
  let editable = false;
  let loadError = "";
  let current = ""; // the .md text as last saved, the base of the next write
  let original = {}; // the texts as loaded, see drawing-format.js
  let embedIds = new Set(); // images the .md links; only these may stay in it
  let latest = null; // elements and files of the last change
  let savedSig = null; // signature of what the file holds
  let source = null; // the file's "source", which Excalidraw would replace with this origin
  let timer = 0;
  const sent = []; // saves not yet answered, oldest first

  const say = (text, error = false) => {
    status.textContent = text;
    status.classList.toggle("error", error);
  };

  // Dark theme: the mode visor sends, else the system's. Shared with note.js.
  const dark = () => {
    const mode = getComputedStyle(document.documentElement).getPropertyValue("--visor-mode").trim();
    return mode ? mode === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
  };

  // Changes to the elements, not to the view: scrolling and zooming do not save.
  const signature = (elements, files) =>
    elements.map((e) => `${e.id}.${e.version}.${e.versionNonce}.${e.isDeleted ? 1 : 0}`).join(";") +
    "|" +
    Object.keys(files).sort().join(",");

  // An image added in a .excalidraw.md could not be saved: the file has no place for it.
  const newImages = (elements) => md && elements.some((e) => e.type === "image" && !e.isDeleted && !embedIds.has(e.fileId));
  const NEW_IMAGES = "Las imágenes nuevas no se guardan en un .excalidraw.md. Bórralas o usa un .excalidraw.";

  function render() {
    root.render(
      React.createElement(Excalidraw, {
        initialData: initial,
        viewModeEnabled: !editing,
        theme: dark() ? "dark" : "light",
        langCode: "es-ES",
        // The page is only the drawing: its keys go to Excalidraw without a click first.
        handleKeyboardGlobally: true,
        UIOptions: { tools: { image: !md }, canvasActions: { loadScene: false } },
        excalidrawAPI: fit,
        onChange,
      }),
    );
  }

  // scrollToContent only centers the scene: a drawing wider than the pane would show
  // a corner. Fit it once, when the canvas first mounts.
  let fitted = false;
  function fit(api) {
    if (fitted || !api) return;
    fitted = true;
    requestAnimationFrame(() => api.scrollToContent(api.getSceneElements(), { fitToViewport: true, viewportZoomFactor: 0.9 }));
  }

  function onChange(elements, appState, files) {
    latest = { elements, appState, files };
    const sig = signature(elements, files);
    // Excalidraw normalizes the scene when it mounts (versions, indexes): that is the
    // baseline, not an edit. Only a change made in edit mode is saved.
    if (!editing) {
      if (!sent.length) savedSig = sig;
      return;
    }
    clearTimeout(timer);
    if (sig === savedSig) return;
    if (newImages(elements)) return say(NEW_IMAGES, true);
    say("Sin guardar");
    timer = setTimeout(save, DELAY);
  }

  // The file's text for the current scene: the drawing block and Text Elements
  // are rewritten, the rest of the .md stays as it was.
  function compose(elements, appState, files) {
    const data = JSON.parse(serializeAsJSON(elements, appState, files, "local"));
    if (source !== null) data.source = source;
    return md ? fmt.writeMd(current, data, LZString, original) : JSON.stringify(data, null, 2);
  }

  function save() {
    clearTimeout(timer);
    if (!latest) return;
    const { elements, appState, files } = latest;
    const sig = signature(elements, files);
    if (sig === savedSig || (sent.length && sig === sent[sent.length - 1].sig)) return;
    if (newImages(elements)) return say(NEW_IMAGES, true);
    let text;
    try {
      text = compose(elements, appState, files);
    } catch (e) {
      return say("No se pudo guardar: " + e.message, true);
    }
    sent.push({ sig, text });
    say("Guardando…");
    send({ type: "save_drawing", text });
  }

  // Rust's answer to a save, in the order they were sent.
  function saved(ok, error) {
    const entry = sent.shift();
    if (!ok) return say("No se pudo guardar: " + error, true);
    if (entry) {
      savedSig = entry.sig;
      if (md) current = entry.text;
    }
    if (sent.length) return say("Guardando…");
    const now = latest ? signature(latest.elements, latest.files) : savedSig;
    if (now === savedSig) return say(editing ? "Guardado" : "");
    say("Sin guardar");
    if (editing) timer = setTimeout(save, DELAY);
  }

  function toggle() {
    if (editing) {
      // Leaving with a new image would drop the change without saying so: stay until it goes.
      if (newImages(latest.elements)) return say(NEW_IMAGES, true);
      save();
      editing = false;
      render();
      if (!sent.length) say("");
      return;
    }
    if (!editable) return say("Este dibujo no se puede editar: " + loadError, true);
    editing = true;
    render();
    say("Editando. Se guarda solo. Ctrl+E para volver a la vista.");
  }

  async function image(id, name) {
    const r = await fetch(location.pathname + "?embed=" + encodeURIComponent(name), { cache: "no-store" });
    if (!r.ok) throw new Error("No se encontró la imagen " + name);
    const blob = await r.blob();
    const dataURL = await new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result);
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(blob);
    });
    return { id, mimeType: blob.type || "image/png", dataURL, created: Date.now() };
  }

  async function load() {
    const text = await fetch(location.pathname + "?raw", { cache: "no-store" }).then((r) => r.text());
    let scene;
    let embeds = [];
    try {
      if (md) {
        const doc = fmt.readMd(text, LZString);
        scene = doc.scene;
        original = doc.original;
        embeds = doc.embeds;
        current = text;
      } else {
        scene = fmt.readJson(text);
      }
    } catch (e) {
      loadError = e.message;
      say("No se pudo abrir el dibujo: " + e.message, true);
      return;
    }
    const { collaborators, ...appState } = scene.appState || {};
    source = scene.source ?? null;
    // URL and LaTeX entries are not loaded (external content, or a math engine visor
    // does not have), but their fileIds are known, so their images are not "new".
    embedIds = new Set(embeds.map((e) => e.id));
    const files = md ? {} : scene.files || {};
    const missing = [];
    const loaded = await Promise.all(
      embeds
        .filter((e) => e.kind === "vault")
        .map(({ id, name }) => image(id, name).catch(() => (missing.push(name), null))),
    );
    for (const f of loaded) if (f) files[f.id] = f;
    if (missing.length) say("Faltan imágenes: " + missing.join(", "), true);

    initial = { elements: scene.elements, appState: { ...appState, viewBackgroundColor: appState.viewBackgroundColor ?? "#ffffff" }, files, scrollToContent: true };
    latest = { elements: scene.elements, appState: initial.appState, files };
    savedSig = signature(scene.elements, files);
    editable = true;
    root = createRoot(mount);
    render();
    addEventListener("visor:tema", render);
  }

  window.__visorDrawing = {
    get editing() {
      return editing;
    },
    toggle,
    saved,
    // The file changed on disk, written by someone else.
    changed() {
      if (editing) say("El archivo cambió fuera de visor. Si sigues editando, lo sobrescribes.", true);
      else location.reload();
    },
    // Before visor hides the tab, which it may unload later: pending changes are saved.
    hide() {
      save();
      return { y: 0 };
    },
  };
  addEventListener("beforeunload", () => save());

  load().catch((e) => say("No se pudo abrir el dibujo: " + e.message, true));
})();
