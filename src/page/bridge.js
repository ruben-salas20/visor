(() => {
  // Only vault pages: websites must not reach the logs and events that agents read.
  if (window.top !== window || window.visor || !location.href.startsWith("__VISOR_BASE__")) return;
  const post = (m) => window.ipc.postMessage(JSON.stringify(m));
  const show = (v) => {
    if (v instanceof Error) return v.stack || String(v);
    if (typeof v === "object" && v !== null) {
      try { return JSON.stringify(v); } catch { return String(v); }
    }
    return String(v);
  };
  // A page that logs every frame would flood visor's main thread. Past the limit
  // lines are dropped before being formatted, and the agent learns how many.
  const LIMIT = 100;
  let since = 0, count = 0, dropped = 0;
  const flush = () => {
    post({ type: "console", level: "warn", text: `Se omitieron ${dropped} mensajes de consola: la página escribió más de ${LIMIT} por segundo.` });
    dropped = 0;
  };
  const allow = () => {
    const now = performance.now();
    if (now - since >= 1000) { since = now; count = 0; }
    if (count++ < LIMIT) return true;
    if (!dropped++) setTimeout(flush, since + 1000 - now);
    return false;
  };
  const log = (level, text) => allow() && post({ type: "console", level, text });

  for (const level of ["log", "info", "warn", "error", "debug"]) {
    const original = console[level];
    console[level] = (...args) => {
      if (allow()) post({ type: "console", level, text: args.map(show).join(" ") });
      original.apply(console, args);
    };
  }

  addEventListener("error", (e) => {
    const el = e.target;
    if (el && el !== window && (el.src || el.href)) {
      log("error", `No se pudo cargar ${el.tagName.toLowerCase()}: ${el.src || el.href}`);
    } else {
      const where = e.filename ? ` (${e.filename}:${e.lineno}:${e.colno})` : "";
      log("error", `${e.message}${where}`);
    }
  }, true);
  addEventListener("unhandledrejection", (e) => log("error", `Promesa rechazada: ${show(e.reason)}`));

  window.visor = Object.freeze({
    send: (data) => post({ type: "event", data: data ?? null }),
  });
})();
