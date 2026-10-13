(() => {
  const tr = (es, en) => ("__VISOR_LANG__" === "en" ? en : es);
  // Only pages visor serves: websites must not reach the logs and events that agents read.
  const served = ["__VISOR_BASE__", "__VISOR_LOOSE_BASE__"].some((base) => location.href.startsWith(base));
  if (window.top !== window || window.visor || !served) return;
  // The app drops messages without this key, so frames from other origins,
  // which can post to the handler but not read this scope, cannot speak for the page.
  const handler = window.webkit.messageHandlers.ipc;
  const KEY = "__VISOR_KEY__";
  // A lone surrogate (a clipped emoji) is valid for JSON.stringify, which writes it
  // as an escape, but serde_json rejects it and the whole message would be lost.
  // Strings are repaired before they are serialized; the escape cannot be fixed after.
  const wellFormed = (_, v) => {
    if (typeof v === "string") return v.toWellFormed();
    if (v && typeof v === "object" && !Array.isArray(v) && Object.keys(v).some((k) => !k.isWellFormed())) {
      return Object.fromEntries(Object.entries(v).map(([k, x]) => [k.toWellFormed(), x]));
    }
    return v;
  };
  const stringify = (v) => JSON.stringify(v, wellFormed);
  const post = (m) => handler.postMessage(stringify({ ...m, key: KEY }));
  Object.defineProperty(window, "__visorPost", { value: post });
  const show = (v) => {
    if (v instanceof Error) return v.stack || String(v);
    if (typeof v === "object" && v !== null) {
      try { return stringify(v); } catch { return String(v); }
    }
    return String(v);
  };
  // A page that logs every frame would flood visor's main thread. Past the limit
  // lines are dropped before being formatted, and the agent learns how many.
  // visor.send has its own counter, so a chatty page cannot starve its console.
  const LIMIT = 100;
  const limiter = (notice) => {
    let since = 0, count = 0, dropped = 0;
    const flush = () => {
      post({ type: "console", level: "warn", text: notice(dropped) });
      dropped = 0;
    };
    return () => {
      const now = performance.now();
      if (now - since >= 1000) { since = now; count = 0; }
      if (count++ < LIMIT) return true;
      if (!dropped++) setTimeout(flush, since + 1000 - now);
      return false;
    };
  };
  const allow = limiter((n) => tr(`Se omitieron ${n} mensajes de consola: la página escribió más de ${LIMIT} por segundo.`, `Skipped ${n} console messages: the page wrote more than ${LIMIT} per second.`));
  const allowEvent = limiter((n) => tr(`Se omitieron ${n} eventos de visor.send: la página envió más de ${LIMIT} por segundo.`, `Skipped ${n} visor.send events: the page sent more than ${LIMIT} per second.`));
  const MAX_EVENT = 16 * 1024;
  const log = (level, text) => allow() && post({ type: "console", level, text });

  for (const level of ["log", "info", "warn", "error", "debug"]) {
    const original = console[level];
    console[level] = (...args) => {
      if (allow()) post({ type: "console", level, text: args.map(show).join(" ") });
      original.apply(console, args);
    };
  }

  // Failed loads of elements and @import rules are not logged here: Rust sees
  // them on the network, where a DOM error event cannot tell which request failed.
  addEventListener("error", (e) => {
    if (e.target && e.target !== window) return;
    const where = e.filename ? ` (${e.filename}:${e.lineno}:${e.colno})` : "";
    log("error", `${e.message}${where}`);
  }, true);
  addEventListener("unhandledrejection", (e) => log("error", tr(`Promesa rechazada: ${show(e.reason)}`, `Unhandled promise rejection: ${show(e.reason)}`)));

  window.visor = Object.freeze({
    send: (data) => {
      if (!allowEvent()) return;
      const json = stringify(data ?? null);
      // Same measure as visor: bytes of the serialized data. Short strings cannot reach it.
      if (json !== undefined && json.length * 3 > MAX_EVENT && new TextEncoder().encode(json).length > MAX_EVENT) {
        log("warn", tr(`Un evento de visor.send se descartó: pasa de ${MAX_EVENT / 1024} KB.`, `A visor.send event was dropped: it is larger than ${MAX_EVENT / 1024} KB.`));
        return;
      }
      post({ type: "event", data: data ?? null });
    },
  });
  // The study kit names no app: its review blocks dispatch this event instead of calling visor.send.
  addEventListener("kit:review", (e) => window.visor.send(e.detail));
})();
