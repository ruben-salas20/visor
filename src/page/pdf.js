// The PDF viewer page (src/vault/pdf.rs). PDF.js is loaded from __visor__/pdfjs/,
// never from a CDN. Scripting is off: a PDF runs no code here.
(function () {
  "use strict";
  const config = window.__visorPdfConfig;
  const logic = window.__visorPdfLogic;
  const texts = config.texts;
  const $ = (id) => document.getElementById(id);

  let pdfViewer = null;
  let total = 0;
  let zoom = 1;
  let fitted = false;

  // Fit to the width of the box, times visor's zoom: Rust keeps WebKit's own
  // zoom at 1 here, so PDF.js draws the canvases sharp at the size shown.
  const fit = () => {
    if (!fitted) return;
    pdfViewer.currentScaleValue = "page-width";
    if (zoom !== 1) pdfViewer.currentScale *= zoom;
  };

  // Set before anything loads, so a script that waits on it can start at once.
  // A rejected `ready` must not raise an unhandled error when nobody waits.
  const loaded = (async () => {
    const lib = await import(config.assets + "pdf.min.mjs");
    // The viewer reads the library from the global object, so it is set first.
    globalThis.pdfjsLib = lib;
    lib.GlobalWorkerOptions.workerSrc = config.assets + "pdf.worker.min.mjs";
    const viewer = await import(config.assets + "pdf_viewer.mjs");
    // No code compiled from the PDF's fonts either: eval is where a PDF once ran script (CVE-2024-4367).
    const doc = await lib.getDocument({ url: config.document, wasmUrl: config.assets + "wasm/", isEvalSupported: false }).promise;
    return { viewer, doc };
  })();
  const ready = loaded.then(({ doc }) => doc.numPages);
  ready.catch(() => {});
  window.__visorPdf = {
    ready,
    goToPage(n) {
      if (!pdfViewer) return;
      const page = logic.parsePage(n, total);
      if (page !== null) pdfViewer.currentPageNumber = page;
    },
    zoom(z) {
      zoom = z;
      document.documentElement.style.setProperty("--visor-zoom", String(z));
      fit();
    },
  };

  loaded.then(setup, () => {
    $("status").textContent = texts.failed;
  });

  async function setup({ viewer, doc }) {
    total = doc.numPages;
    const eventBus = new viewer.EventBus();
    const linkService = new viewer.PDFLinkService({ eventBus });
    const findController = new viewer.PDFFindController({ linkService, eventBus, updateMatchesCountOnProgress: true });
    pdfViewer = new viewer.PDFViewer({
      container: $("contenedor"),
      viewer: $("visor"),
      eventBus,
      linkService,
      findController,
      l10n: new viewer.GenericL10n(config.lang),
      enableScripting: false,
      textLayerMode: 1,
      imageResourcesPath: config.assets + "images/",
    });
    linkService.setViewer(pdfViewer);
    pdfViewer.setDocument(doc);
    linkService.setDocument(doc, null);

    eventBus.on("pagesinit", () => {
      fitted = true;
      fit();
    });
    // Only a change of width refits: the height changes when the find bar opens.
    let width = 0;
    new ResizeObserver(([entry]) => {
      if (entry.contentRect.width === width) return;
      width = entry.contentRect.width;
      fit();
    }).observe($("contenedor"));

    setupPages(eventBus);
    setupText(eventBus);
    setupFind(eventBus);
    setupOutline(doc, linkService);
    $("abrir-fuera").onclick = () => window.__visorPost({ type: "open_pdf_elsewhere" });
  }

  function setupPages(eventBus) {
    const input = $("pagina");
    const prev = $("anterior");
    const next = $("siguiente");
    $("total").textContent = texts.ofPages.replace("{m}", total);
    const show = (n) => {
      input.value = String(n);
      input.setAttribute("aria-label", logic.pageLabel(texts.pageOf, n, total));
      prev.disabled = n <= 1;
      next.disabled = n >= total;
    };
    eventBus.on("pagechanging", ({ pageNumber }) => show(pageNumber));
    show(1);
    input.addEventListener("change", () => {
      const page = logic.parsePage(input.value, total);
      if (page === null) show(pdfViewer.currentPageNumber);
      else pdfViewer.currentPageNumber = page;
    });
    prev.onclick = () => {
      pdfViewer.currentPageNumber = Math.max(1, pdfViewer.currentPageNumber - 1);
    };
    next.onclick = () => {
      pdfViewer.currentPageNumber = Math.min(total, pdfViewer.currentPageNumber + 1);
    };
  }

  // One event per text layer drawn: the note's annotations wait for it. The
  // event carries the page only; it fires on failure too, as PDF.js does.
  function setupText(eventBus) {
    eventBus.on("textlayerrendered", ({ pageNumber }) => {
      document.dispatchEvent(new CustomEvent("visor:pdf-textlayer", { detail: { page: pageNumber } }));
    });
  }

  function setupFind(eventBus) {
    const bar = $("buscar");
    const input = $("texto");
    let lastQuery = null;
    const search = (query, findPrevious) => {
      const again = query === lastQuery;
      lastQuery = query;
      eventBus.dispatch("find", {
        source: window,
        type: again ? "again" : "",
        query,
        caseSensitive: false,
        entireWord: false,
        highlightAll: true,
        findPrevious,
        matchDiacritics: false,
      });
    };
    const open = () => {
      bar.hidden = false;
      input.focus();
      input.select();
    };
    const close = () => {
      bar.hidden = true;
      input.value = "";
      $("cuenta").textContent = "";
      lastQuery = null;
      search("", false);
    };
    eventBus.on("updatefindmatchescount", ({ matchesCount }) => {
      $("cuenta").textContent = logic.matchLabel(texts.matches, texts.noMatches, matchesCount.current, matchesCount.total);
    });
    bar.addEventListener("submit", (e) => e.preventDefault());
    // Enter goes to the next match, Shift+Enter to the previous one.
    input.addEventListener("keydown", (e) => {
      if (e.key !== "Enter") return;
      e.preventDefault();
      search(input.value, e.shiftKey);
    });
    input.addEventListener("input", () => search(input.value, false));
    $("buscar-antes").onclick = () => search(input.value, true);
    $("buscar-despues").onclick = () => search(input.value, false);
    $("buscar-cerrar").onclick = close;
    $("buscar-boton").onclick = open;
    document.addEventListener("keydown", (e) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "f") {
        // The page takes Ctrl+F: the webview would otherwise do nothing with it.
        e.preventDefault();
        open();
      } else if (e.key === "Escape" && !bar.hidden) {
        close();
      }
    });
  }

  async function setupOutline(doc, linkService) {
    const outline = await doc.getOutline();
    if (!logic.hasOutline(outline)) return;
    const panel = $("esquema");
    const button = $("esquema-boton");
    button.hidden = false;
    button.onclick = () => {
      panel.hidden = !panel.hidden;
      button.setAttribute("aria-expanded", String(!panel.hidden));
    };
    const add = (items, list) => {
      for (const item of items) {
        const li = document.createElement("li");
        if (item.dest) {
          const entry = document.createElement("button");
          entry.type = "button";
          entry.textContent = item.title;
          entry.onclick = () => linkService.goToDestination(item.dest);
          li.append(entry);
        } else {
          li.textContent = item.title;
        }
        if (item.items?.length) {
          const sub = document.createElement("ul");
          add(item.items, sub);
          li.append(sub);
        }
        list.append(li);
      }
    };
    add(outline, $("esquema-lista"));
  }
})();
