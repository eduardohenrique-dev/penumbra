// Bolhas de vidro flutuando no fundo. Só `transform` é animado (barato);
// as animações pausam quando a janela está escondida ou sem foco.
const Bubbles = (() => {
  const COUNT = 10;
  const items = [];
  let host;

  function rand(a, b) { return a + Math.random() * (b - a); }

  function init(el) {
    host = el;
    const w = host.clientWidth || 420, h = host.clientHeight || 560;
    for (let i = 0; i < COUNT; i++) {
      const size = rand(18, 64);
      const x = rand(0, w - size), y = rand(0, h - size);
      const outer = document.createElement('div');
      outer.className = 'bubble';
      outer.style.cssText = `left:${(x / w * 100).toFixed(2)}%;top:${(y / h * 100).toFixed(2)}%;width:${size}px;height:${size}px;opacity:${rand(.55, .95).toFixed(2)}`;
      const inner = document.createElement('div');
      inner.className = 'b';
      inner.style.setProperty('--dx', `${rand(-26, 26).toFixed(0)}px`);
      inner.style.setProperty('--dy', `${rand(-46, -14).toFixed(0)}px`);
      inner.style.setProperty('--dur', `${rand(7, 14).toFixed(1)}s`);
      inner.style.setProperty('--spin', `${rand(9, 22).toFixed(1)}s`);
      inner.style.setProperty('--delay', `${(-rand(0, 10)).toFixed(1)}s`);
      outer.appendChild(inner);
      host.appendChild(outer);
      items.push({ outer, fx: x / w, fy: y / h, r: size / 2 });
    }
  }

  // Afasta as bolhas próximas do ponto (x, y) em coordenadas da janela.
  function push(x, y, strength = 1) {
    const W = host.clientWidth, H = host.clientHeight;
    for (const b of items) {
      const cx = b.fx * W + b.r, cy = b.fy * H + b.r;
      const dx = cx - x, dy = cy - y;
      const dist = Math.hypot(dx, dy) || 1;
      const reach = 170 + b.r;
      const k = Math.max(0, 1 - dist / reach);
      if (k <= 0) { b.outer.style.transform = ''; continue; }
      const m = k * 46 * strength;
      b.outer.style.transform = `translate3d(${(dx / dist) * m}px, ${(dy / dist) * m}px, 0) scale(${1 + k * .22 * strength}, ${1 - k * .12 * strength})`;
    }
  }

  function release() { for (const b of items) b.outer.style.transform = ''; }

  function setPaused(p) { document.body.classList.toggle('paused', p); }

  return { init, push, release, setPaused };
})();
