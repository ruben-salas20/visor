// The PDF viewer page (src/vault/pdf.rs): the full viewer of PDF.js, the one in
// Firefox, loaded from __visor__/pdfjs/ and never from a CDN. Scripting is off:
// a PDF runs no code here. Its editing tools are off too: visor's annotations
// (annot.js) mark the text instead.
(function () {
  "use strict";
  const config = window.__visorPdfConfig;
  const assets = config.assets;

  let app = null;
  let ready = null;
  const loaded = new Promise((resolve, reject) => {
    ready = { resolve, reject };
  });
  loaded.catch(() => {});
  window.__visorPdf = {
    // Resolves with the number of pages once the document is laid out.
    ready: loaded,
    goToPage(n) {
      const page = Number(n);
      if (app?.pdfDocument && Number.isInteger(page) && page >= 1 && page <= app.pagesCount) app.page = page;
    },
    // Ctrl+plus, Ctrl+minus and Ctrl+0, sent by Rust: they never reach the page.
    zoom(step) {
      if (!app?.pdfViewer) return;
      if (step > 0) app.zoomIn();
      else if (step < 0) app.zoomOut();
      else app.zoomReset();
    },
  };

  const failed = () => {
    document.body.textContent = config.texts.failed;
    ready.reject(new Error(config.texts.failed));
  };

  // The viewer opens `?file=` instead of its default when the address has it:
  // the page would then show a PDF that is not the one its notes belong to.
  if (new URLSearchParams(location.search).has("file")) {
    failed();
    return;
  }

  // visor's theme, not the system's: the viewer's colours follow color-scheme.
  const theme = (mode) => {
    if (mode === "dark" || mode === "light") document.documentElement.style.colorScheme = mode;
  };
  theme(getComputedStyle(document.documentElement).getPropertyValue("--visor-mode").trim());
  addEventListener("visor:tema", (e) => theme(e.detail?.modo));

  // Read by the viewer before it starts. Preferences are off so that nothing
  // saved by the viewer can turn scripting or the editors back on.
  document.addEventListener("webviewerloaded", () => {
    const options = window.PDFViewerApplicationOptions;
    options.set("disablePreferences", true);
    options.setAll({
      defaultUrl: config.document,
      workerSrc: assets + "pdf.worker.min.mjs",
      wasmUrl: assets + "wasm/",
      imageResourcesPath: assets + "images/",
      enableScripting: false,
      annotationEditorMode: -1,
      enableSignatureEditor: false,
      enableComment: false,
      enableAltText: false,
      pdfBugEnabled: false,
      // The page's address stays the one visor loaded: no #page= in its history.
      disableHistory: true,
      localeProperties: { lang: config.locale },
    });
  });

  start().catch(failed);

  async function start() {
    const markup = await fetch(assets + "viewer-body.html").then((r) => {
      if (!r.ok) throw new Error(String(r.status));
      return r.text();
    });
    document.body.insertAdjacentHTML("afterbegin", markup);
    addOpenElsewhere();

    // The viewer reads the library from the global object, so it is set first.
    // Its options take no null for these folders, which are not packed
    // (tools/pdfjs/README.md): without them, nothing is asked for.
    const lib = await import(assets + "pdf.min.mjs");
    const unpacked = { cMapUrl: null, standardFontDataUrl: null, iccUrl: null };
    globalThis.pdfjsLib = { ...lib, getDocument: (params) => lib.getDocument({ ...params, ...unpacked }) };
    lib.GlobalWorkerOptions.workerSrc = assets + "pdf.worker.min.mjs";
    await import(assets + "viewer.mjs");

    app = window.PDFViewerApplication;
    await app.initializedPromise;
    const bus = app.eventBus;
    bus.on("pagesinit", () => ready.resolve(app.pagesCount));
    // One event per text layer drawn: the note's annotations wait for it. The
    // event carries the page only; it fires on failure too, as PDF.js does.
    bus.on("textlayerrendered", ({ pageNumber }) => {
      document.dispatchEvent(new CustomEvent("visor:pdf-textlayer", { detail: { page: pageNumber } }));
    });
  }

  // First in the viewer's «more tools» menu, where its own «Open» was.
  function addOpenElsewhere() {
    const menu = document.getElementById("secondaryToolbarButtonContainer");
    if (!menu) return;
    const button = document.createElement("button");
    button.id = "visorOpenElsewhere";
    button.type = "button";
    button.className = "toolbarButton labeled";
    const label = document.createElement("span");
    label.textContent = config.texts.openElsewhere;
    button.append(label);
    button.addEventListener("click", () => window.__visorPost({ type: "open_pdf_elsewhere" }));
    menu.prepend(button);
  }
})();
