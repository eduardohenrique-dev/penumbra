// Penumbra — interface. Fala com o backend Rust pelos comandos Tauri;
// fora do Tauri (pré-visualização no navegador) usa monitores fictícios.
const MAX = 90;
const tauri = window.__TAURI__;

const mock = (() => {
  const mons = [
    { id: 'm1', index: 1, name: 'Monitor principal', primary: true, x: 0, y: 0, width: 2560, height: 1440, level: 35, enabled: true, ddc: true },
    { id: 'm2', index: 2, name: 'Monitor lateral', primary: false, x: 2560, y: 0, width: 1920, height: 1080, level: 0, enabled: false, ddc: false },
  ];
  return {
    list_monitors: async () => mons.map((m) => ({ ...m })),
    set_level: async ({ id, level }) => { const m = mons.find((x) => x.id === id); m.level = level; m.enabled = level > 0 || m.enabled; },
    set_enabled: async ({ id, enabled }) => { mons.find((x) => x.id === id).enabled = enabled; },
    hide_main: async () => {},
    ui_ready: async () => {},
    fit_window: async () => {},
    set_all: async ({ level }) => { mons.forEach((m) => { m.level = level; m.enabled = level > 0 || m.enabled; }); },
  };
})();

const invoke = tauri ? (cmd, args) => tauri.core.invoke(cmd, args) : (cmd, args) => mock[cmd](args || {});

/* ---------- Lente de vidro: mapa de deslocamento gerado em canvas ---------- */
function buildLensMap() {
  const w = 116, h = 68, c = document.createElement('canvas');
  c.width = w; c.height = h;
  const ctx = c.getContext('2d');
  const img = ctx.createImageData(w, h);
  const r = h / 2, edge = r * 0.75;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      // distância assinada ao "pill" (0 na borda, cresce para dentro)
      const cx = Math.min(Math.max(x + .5, r), w - r), cy = h / 2;
      const dx = x + .5 - cx, dy = y + .5 - cy;
      const dist = Math.hypot(dx, dy);
      const inside = r - dist;
      let t = Math.max(0, 1 - inside / edge);
      t = inside < 0 ? 0 : t * t;
      const nx = dist ? dx / dist : 0, ny = dist ? dy / dist : 0;
      const i = (y * w + x) * 4;
      img.data[i] = 128 + nx * t * 127;
      img.data[i + 1] = 128 + ny * t * 127;
      img.data[i + 2] = 128;
      img.data[i + 3] = 255;
    }
  }
  ctx.putImageData(img, 0, 0);
  document.getElementById('lensMap').setAttribute('href', c.toDataURL('image/png'));
}

/* ---------- Estado e renderização ---------- */
const listEl = document.getElementById('list');
const cards = new Map(); // id -> { el, set(view) }
let dragging = false;

function resolution(m) { return `${m.width}×${m.height}`; }

function createCard(m) {
  const el = document.createElement('section');
  el.className = 'card';
  el.innerHTML = `
    <div class="row">
      <div class="glyph"></div>
      <div class="meta"><div class="name"></div><div class="sub"></div></div>
      <button class="switch" role="switch" aria-label="Ativar escurecimento"></button>
    </div>
    <div class="control">
      <div class="slider" tabindex="0" role="slider" aria-valuemin="0" aria-valuemax="${MAX}">
        <div class="track"><div class="fill"></div></div>
        <div class="thumb"></div>
      </div>
      <div class="value"><span class="num">0</span><small>%</small></div>
    </div>
    <div class="chips">
      <button class="chip" data-v="25">25%</button>
      <button class="chip" data-v="50">50%</button>
      <button class="chip" data-v="75">75%</button>
      <span class="tag"><i></i><span class="tagtxt"></span></span>
    </div>`;

  const $ = (s) => el.querySelector(s);
  const slider = $('.slider'), fill = $('.fill'), thumb = $('.thumb'), num = $('.num'), sw = $('.switch');
  const state = { id: m.id, level: m.level, enabled: m.enabled };
  let pending = null, raf = 0;

  const THUMB_W = 40;
  const chips = [...el.querySelectorAll('.chip')];
  let placed = false;
  function paint() {
    const w = slider.clientWidth;
    if (w > THUMB_W) {
      // Antes de entrar na tela o slider mede 0; e o primeiro posicionamento não deve deslizar.
      if (!placed) thumb.style.transition = 'none';
      const x = (state.level / MAX) * (w - THUMB_W);
      thumb.style.transform = `translate3d(${x}px,0,0) scale(${dragging ? 1.18 : 1}, ${dragging ? 1.12 : 1})`;
      fill.style.width = `${x + THUMB_W / 2}px`;
      if (!placed) {
        placed = true;
        requestAnimationFrame(() => (thumb.style.transition = ''));
      }
    }
    num.textContent = state.level;
    slider.setAttribute('aria-valuenow', state.level);
    slider.setAttribute('aria-valuetext', state.enabled ? `${state.level}%` : 'Desligado');
    sw.setAttribute('aria-checked', state.enabled);
    el.classList.toggle('off', !state.enabled);
    for (const c of chips) {
      const on = state.enabled && state.level === +c.dataset.v;
      c.classList.toggle('on', on);
      c.setAttribute('aria-pressed', on);
    }
    markAll();
  }

  function send(level) {
    pending = level;
    if (!raf) raf = requestAnimationFrame(() => {
      raf = 0;
      invoke('set_level', { id: state.id, level: pending });
    });
  }

  function setLevel(v, push = true) {
    v = Math.max(0, Math.min(MAX, Math.round(v)));
    state.level = v;
    if (v > 0) state.enabled = true;
    paint();
    if (push) send(v);
  }

  function fromPointer(e) {
    const rect = slider.getBoundingClientRect();
    const x = e.clientX - rect.left - THUMB_W / 2;
    setLevel((x / (rect.width - THUMB_W)) * MAX);
    const sr = document.getElementById('shell').getBoundingClientRect();
    Bubbles.push(e.clientX - sr.left, rect.top - sr.top + rect.height / 2);
  }

  slider.addEventListener('pointerdown', (e) => {
    slider.setPointerCapture(e.pointerId);
    dragging = true; slider.classList.add('dragging');
    fromPointer(e);
  });
  slider.addEventListener('pointermove', (e) => { if (dragging) fromPointer(e); });
  const end = () => {
    if (!dragging) return;
    dragging = false; slider.classList.remove('dragging'); paint(); Bubbles.release();
  };
  slider.addEventListener('pointerup', end);
  slider.addEventListener('pointercancel', end);
  slider.addEventListener('keydown', (e) => {
    const step = e.shiftKey ? 10 : 1;
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') { setLevel(state.level + step); e.preventDefault(); }
    if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') { setLevel(state.level - step); e.preventDefault(); }
    if (e.key === 'Home') { setLevel(0); e.preventDefault(); }
    if (e.key === 'End') { setLevel(MAX); e.preventDefault(); }
  });

  sw.addEventListener('click', () => {
    state.enabled = !state.enabled;
    if (state.enabled && state.level === 0) state.level = 30;
    paint();
    invoke('set_enabled', { id: state.id, enabled: state.enabled }).then(() => {
      if (state.enabled) invoke('set_level', { id: state.id, level: state.level });
    });
  });
  chips.forEach((c) => c.addEventListener('click', () => setLevel(+c.dataset.v)));

  function set(view) {
    $('.glyph').textContent = view.index;
    $('.name').textContent = `Tela ${view.index}${view.primary ? ' · Principal' : ''}`;
    slider.setAttribute('aria-label', `Escurecimento da tela ${view.index}`);
    sw.setAttribute('aria-label', `Escurecer a tela ${view.index}`);
    const friendly = view.name && !/generic|pnp/i.test(view.name) ? ' · ' + view.name : '';
    const portrait = view.height > view.width ? ' · Vertical' : '';
    $('.sub').textContent = `${resolution(view)}${portrait}${friendly}`;
    const tag = $('.tag');
    tag.classList.toggle('soft', !view.ddc);
    $('.tagtxt').textContent = view.ddc ? 'Brilho real + camada' : 'Camada';
    if (!dragging) { state.level = view.level; state.enabled = view.enabled; }
    paint();
  }

  new ResizeObserver(paint).observe(slider);
  set(m);
  return { el, set, state };
}

/** Acende o atalho de "Todas as telas" que corresponde ao estado atual de todas elas. */
function markAll() {
  const states = [...cards.values()].map((c) => c.state);
  document.querySelectorAll('[data-all]').forEach((b) => {
    const v = +b.dataset.all;
    const on = states.length > 0 && states.every((s) => (v === 0 ? !s.enabled || s.level === 0 : s.enabled && s.level === v));
    b.classList.toggle('on', on);
    b.setAttribute('aria-pressed', on);
  });
}

/* Altura natural do conteúdo (px lógicos): a janela cresce/encolhe com o número de monitores. */
function naturalHeight() {
  const px = (v) => parseFloat(v) || 0;
  const shell = document.getElementById('shell'), cs = getComputedStyle(shell);
  const header = shell.querySelector('.top'), foot = document.getElementById('foot');
  const lcs = getComputedStyle(listEl);
  const kids = [...listEl.children];
  const body = kids.reduce((s, c) => s + c.offsetHeight, 0) + px(lcs.rowGap) * Math.max(0, kids.length - 1)
    + px(lcs.paddingTop) + px(lcs.paddingBottom);
  return Math.ceil(px(cs.paddingTop) + px(cs.paddingBottom) + 2 + header.offsetHeight + px(getComputedStyle(header).marginBottom)
    + body + foot.offsetHeight + px(getComputedStyle(foot).marginTop));
}
function fit() { return invoke('fit_window', { height: naturalHeight() }); }

async function load() {
  let views = [];
  try { views = await invoke('list_monitors'); } catch (e) { console.error(e); }
  const ids = new Set(views.map((v) => v.id));
  for (const [id, c] of cards) if (!ids.has(id)) { c.el.remove(); cards.delete(id); }
  listEl.querySelector('.empty')?.remove();
  if (!views.length) {
    const d = document.createElement('div');
    d.className = 'empty';
    d.textContent = 'Nenhum monitor encontrado.';
    listEl.appendChild(d);
    await fit();
    return;
  }
  document.body.classList.toggle('single', views.length < 2);
  for (const v of views) {
    let c = cards.get(v.id);
    if (!c) { c = createCard(v); cards.set(v.id, c); }
    c.set(v);
    listEl.appendChild(c.el); // mantém a ordem
  }
  markAll();
  await fit();
}

/* ---------- Ciclo de vida ---------- */
function updateRunning() {
  const active = !document.hidden && document.hasFocus();
  Bubbles.setPaused(!active);
}

/* ---------- Tema (Noite / Aurora) ---------- */
const THEMES = ['noite', 'aurora'];
function setTheme(t) {
  document.documentElement.dataset.theme = t;
  const name = t === 'noite' ? 'Noite' : 'Aurora';
  document.getElementById('theme').title = `Tema: ${name}`;
  document.getElementById('theme').setAttribute('aria-label', `Trocar tema (atual: ${name})`);
  try { localStorage.setItem('penumbra-theme', t); } catch {}
}
let savedTheme = 'noite';
try { savedTheme = localStorage.getItem('penumbra-theme') || 'noite'; } catch {}
setTheme(THEMES.includes(savedTheme) ? savedTheme : 'noite');
document.getElementById('theme').addEventListener('click', () => {
  const cur = document.documentElement.dataset.theme;
  setTheme(THEMES[(THEMES.indexOf(cur) + 1) % THEMES.length]);
});

/* ---------- Mover e redimensionar ---------- */
const appWindow = tauri?.window?.getCurrentWindow?.();
const shellEl = document.getElementById('shell');
const isDragArea = (t) => !t.closest?.('button, .slider, .switch') && (
  t === shellEl || t === listEl || t.classList?.contains('backdrop') || t.closest?.('.foot') ||
  t.closest?.('.top'));

shellEl.addEventListener('mousedown', (e) => {
  if (e.button !== 0 || !appWindow) return;
  const dir = e.target.dataset?.resize;
  if (dir) { e.preventDefault(); appWindow.startResizeDragging(dir); return; }
  if (isDragArea(e.target)) appWindow.startDragging();
});

// Janela mais larga = interface ampliada (zoom); a altura acompanha o conteúdo depois que o
// usuário para de puxar a borda (sem brigar com o arrasto).
let fitTimer = 0;
if (tauri?.event) tauri.event.listen('zoom-changed', () => {
  clearTimeout(fitTimer);
  fitTimer = setTimeout(fit, 350);
});

document.querySelectorAll('[data-all]').forEach((b) =>
  b.addEventListener('click', async () => { await invoke('set_all', { level: +b.dataset.all }); load(); }));

document.getElementById('close').addEventListener('click', () => invoke('hide_main'));
document.addEventListener('visibilitychange', () => { updateRunning(); if (!document.hidden) load(); });
window.addEventListener('focus', updateRunning);
window.addEventListener('blur', updateRunning);
window.addEventListener('keydown', (e) => { if (e.key === 'Escape') invoke('hide_main'); });

if (tauri?.event) tauri.event.listen('monitors-changed', () => { if (!dragging) load(); });

buildLensMap();
Bubbles.init(document.getElementById('bubbles'));
load().finally(() => invoke('ui_ready'));
updateRunning();
