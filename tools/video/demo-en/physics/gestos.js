// Demo only (tools/video/demo.sh): plays the person's gestures, which cannot be sent
// from outside: select a phrase, press "Comentar", type a question, save, and reopen the note.
// Runs only if gestos.on exists next to the page (demo.sh creates it in its copy of the
// vault). A drawn pointer stands in for the mouse, which screenshots do not show.
(() => {
  const cfg = document.currentScript.dataset;
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const layer = () => document.querySelector("visor-capa")?.shadowRoot;

  const ptr = document.createElement("div");
  ptr.setAttribute("data-visor-skip", "");
  ptr.style.cssText = "position:fixed;left:0;top:0;width:22px;height:22px;z-index:2147483647;pointer-events:none;transition:opacity .3s;filter:drop-shadow(0 1px 2px rgb(0 0 0/.5))";
  ptr.innerHTML = '<svg viewBox="0 0 22 22" width="22" height="22"><path d="M3 2l14 10.5-6.2.8 3.6 6.9-2.6 1.3-3.6-6.9L3 19z" fill="#fff" stroke="#111" stroke-width="1.3" stroke-linejoin="round"/></svg>';
  let px = innerWidth - 40, py = innerHeight - 30;
  const put = (x, y) => { px = x; py = y; ptr.style.transform = `translate(${x - 3}px, ${y - 2}px)`; };
  const top = () => document.documentElement.append(ptr); // stay above visor's layer

  async function move(x, y, ms = 500) {
    const x0 = px, y0 = py, t0 = performance.now();
    for (;;) {
      const k = Math.min(1, (performance.now() - t0) / ms);
      const e = k < .5 ? 2 * k * k : 1 - (-2 * k + 2) ** 2 / 2;
      put(x0 + (x - x0) * e, y0 + (y - y0) * e);
      if (k === 1) return;
      await new Promise(requestAnimationFrame);
    }
  }
  const center = (el) => { const r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; };
  const mouse = (type, x, y) => (document.elementFromPoint(x, y) || document.body)
    .dispatchEvent(new MouseEvent(type, { bubbles: true, composed: true, clientX: x, clientY: y, button: 0 }));
  async function press(el) { const [x, y] = center(el); await move(x, y, 450); await sleep(150); el.click(); }

  function textRange(phrase) {
    const walk = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let n; (n = walk.nextNode());) {
      const i = n.data.indexOf(phrase);
      if (i >= 0) return [n, i, i + phrase.length];
    }
    throw new Error("frase no encontrada: " + phrase);
  }
  const caret = (node, off) => { const r = document.createRange(); r.setStart(node, off); r.setEnd(node, off); return r.getBoundingClientRect(); };

  async function run() {
    put(px, py);
    top();
    await sleep(1200);
    const [node, start, end] = textRange(cfg.frase);
    let r = caret(node, start);
    await move(r.left, r.top + r.height / 2, 700);
    mouse("mousedown", px, py);
    const sel = getSelection();
    for (let i = start + 1; i <= end; i++) {
      sel.setBaseAndExtent(node, start, node, i);
      r = caret(node, i);
      put(r.left, r.top + r.height / 2);
      await sleep(900 / (end - start));
    }
    mouse("mouseup", px, py);
    await sleep(400);
    top();
    await sleep(500);
    await press(layer().querySelector(".toolbar .text"));
    top();
    await sleep(300);
    const area = layer().querySelector(".pop textarea");
    for (const ch of cfg.pregunta) {
      area.value += ch;
      area.dispatchEvent(new Event("input", { bubbles: true }));
      await sleep(55);
    }
    await sleep(500);
    await press(layer().querySelector(".pop .actions .primary"));
    await sleep(900);
    // Reopen the note to watch the answer arrive.
    const mark = textRange(cfg.frase);
    r = caret(mark[0], mark[1] + 6);
    await move(r.left, r.top + r.height / 2, 450);
    await sleep(150);
    mouse("click", px, py);
    top();
    await sleep(300);
    await move(innerWidth - 60, innerHeight - 40, 700);
    ptr.style.opacity = "0";
  }
  addEventListener("load", async () => {
    const on = await fetch("gestos.on", { cache: "no-store" }).then((r) => r.ok, () => false);
    if (on) run().catch((e) => console.error(e));
  }, { once: true });
})();
