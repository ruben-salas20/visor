// Injected into the frame's main frame only, before the frame's own code.
// The key lives in this closure: Rust drops frame messages without it.
(() => {
  const ipc = window.ipc;
  const KEY = "__VISOR_FRAME_KEY__";
  const post = (m) => ipc.postMessage(JSON.stringify({ ...m, key: KEY }));
  // Snapshots that arrive before React listens wait here.
  const waiting = [];
  let listener = null;
  const receive = (m) => (listener ? listener(m) : waiting.push(m));
  const listen = (f) => {
    listener = f;
    while (waiting.length) f(waiting.shift());
  };
  Object.defineProperty(window, "__visorFrame", { value: Object.freeze({ post, receive, listen }) });
})();
