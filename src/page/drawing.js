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
  const pending = new Set(); // image ids sent to Rust, not answered yet
  const failed = new Map(); // image id -> Rust's reason, for images that were not saved
  const added = new Map(); // image id -> file name, for images saved in this session

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
  // Files the .md links are left out: a web image arriving late is not a change.
  const signature = (elements, files) =>
    elements.map((e) => `${e.id}.${e.version}.${e.versionNonce}.${e.isDeleted ? 1 : 0}`).join(";") +
    "|" +
    Object.keys(files).filter((id) => !embedIds.has(id)).sort().join(",");

  // An image added to a .excalidraw.md goes to the vault first (sendImages), and the
  // drawing links it only once Rust has saved it (imageSaved). Until then, or while it
  // failed, the drawing does not save: the file would lose the image on reload.
  const unsaved = (elements) => (md ? elements.filter((e) => e.type === "image" && !e.isDeleted && !embedIds.has(e.fileId)) : []);

  // What holds the drawing back, as the status bar says it, or null.
  function waiting(elements) {
    const list = unsaved(elements);
    if (!list.length) return null;
    const failure = list.find((e) => failed.has(e.fileId));
    if (failure) return { text: "No se pudo guardar la imagen: " + failed.get(failure.fileId).replace(/\.$/, "") + ". Bórrala del dibujo para seguir.", error: true };
    return { text: "Guardando la imagen…", error: false };
  }

  // Each new image is sent once. A failed one is not sent again: deleting it lifts the wait.
  function sendImages(elements, files) {
    for (const e of unsaved(elements)) {
      const data = files[e.fileId]?.dataURL;
      if (!data || pending.has(e.fileId) || failed.has(e.fileId)) continue;
      pending.add(e.fileId);
      send({ type: "save_image", file_id: e.fileId, data: data.slice(data.indexOf(",") + 1) });
    }
  }

  function render() {
    root.render(
      React.createElement(Excalidraw, {
        initialData: initial,
        viewModeEnabled: !editing,
        theme: dark() ? "dark" : "light",
        langCode: "es-ES",
        // The page is only the drawing: its keys go to Excalidraw without a click first.
        handleKeyboardGlobally: true,
        UIOptions: { tools: { image: true }, canvasActions: { loadScene: false } },
        excalidrawAPI: fit,
        onChange,
      }),
    );
  }

  // scrollToContent only centers the scene: a drawing wider than the pane would show
  // a corner. Fit it once, when the canvas first mounts.
  let fitted = false;
  let gotApi;
  const api = new Promise((resolve) => (gotApi = resolve));
  function fit(excalidraw) {
    if (excalidraw) gotApi(excalidraw);
    if (fitted || !excalidraw) return;
    fitted = true;
    requestAnimationFrame(() => excalidraw.scrollToContent(excalidraw.getSceneElements(), { fitToViewport: true, viewportZoomFactor: 0.9 }));
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
    sendImages(elements, files);
    if (sig === savedSig) return;
    const wait = waiting(elements);
    if (wait) return say(wait.text, wait.error);
    say("Sin guardar");
    timer = setTimeout(save, DELAY);
  }

  // The file's text for the current scene: the drawing block and Text Elements
  // are rewritten, the rest of the .md stays as it was.
  function compose(elements, appState, files) {
    const data = JSON.parse(serializeAsJSON(elements, appState, files, "local"));
    if (source !== null) data.source = source;
    if (!md) return JSON.stringify(data, null, 2);
    // The images saved in this session are linked here, before the pruning of writeMd
    // drops the ones deleted from the scene. addEmbed skips those already linked.
    let base = current;
    for (const [id, name] of added) base = fmt.addEmbed(base, id, name);
    return fmt.writeMd(base, data, LZString, original);
  }

  function save() {
    clearTimeout(timer);
    if (!latest) return;
    const { elements, appState, files } = latest;
    const sig = signature(elements, files);
    if (sig === savedSig || (sent.length && sig === sent[sent.length - 1].sig)) return;
    const wait = waiting(elements);
    if (wait) return say(wait.text, wait.error);
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

  // Rust's answer to an image: saved under `name`, or refused with `error`.
  function imageSaved(fileId, ok, name, error) {
    pending.delete(fileId);
    if (!ok) {
      failed.set(fileId, error);
      const wait = latest && waiting(latest.elements);
      if (wait) say(wait.text, wait.error);
      return;
    }
    failed.delete(fileId);
    embedIds.add(fileId);
    added.set(fileId, name);
    if (!editing || !latest) return;
    const wait = waiting(latest.elements);
    if (wait) return say(wait.text, wait.error);
    say("Sin guardar");
    clearTimeout(timer);
    timer = setTimeout(save, DELAY);
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
      const wait = waiting(latest.elements);
      if (wait) return say(wait.text, wait.error);
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

  // A web image is handed to Excalidraw by its address: it makes an <img> from it, so the
  // browser fetches the bytes and no script reads them. The address is first loaded in
  // an <img> here: Excalidraw asks again for a file it could not load, and each ask is a
  // request (a dead address flooded the console and the network). "image/png" for every type:
  // Excalidraw decodes the source of an SVG as data, which a URL is not.
  function webImage(e) {
    const url = fmt.imageUrl(e);
    if (!url) return null;
    return new Promise((resolve) => {
      const probe = new Image();
      probe.onload = () => resolve({ id: e.id, mimeType: "image/png", dataURL: url, created: Date.now() });
      probe.onerror = () => resolve(null);
      probe.src = url;
    });
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
    // LaTeX entries are not loaded (a math engine visor does not have), but their fileIds
    // are known, so their images are not "new".
    embedIds = new Set(embeds.map((e) => e.id));
    const files = md ? {} : scene.files || {};
    const missing = [];
    const loaded = await Promise.all(
      embeds.filter((e) => e.kind === "vault").map(({ id, name }) => image(id, name).catch(() => (missing.push(name), null))),
    );
    // Web images are added once Excalidraw has its API: a slow site must not hold
    // the drawing back.
    for (const e of embeds) {
      api.then(async (excalidraw) => {
        const f = await webImage(e);
        if (f) excalidraw.addFiles([f]);
      });
    }
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
    imageSaved,
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
