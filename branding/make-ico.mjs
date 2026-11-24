// Gera um .ico "clássico" compatível com o Explorer: tamanhos pequenos em bitmap 32 bpp (BGRA)
// e só o de 256 px em PNG. O .ico do `tauri icon` usa PNG em todos os tamanhos, o que faz
// o Explorer mostrar o ícone genérico em atalhos.
// Uso: node branding/make-ico.mjs <saida.ico>
import { Resvg } from '@resvg/resvg-js';
import { readFileSync, writeFileSync } from 'node:fs';

const svg = readFileSync(new URL('./logo.svg', import.meta.url), 'utf8');
const out = process.argv[2] ?? new URL('../src-tauri/icons/icon.ico', import.meta.url);
const SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];

function dib(size) {
  const r = new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render();
  const rgba = r.pixels;
  const header = Buffer.alloc(40);
  header.writeUInt32LE(40, 0);          // biSize
  header.writeInt32LE(size, 4);         // largura
  header.writeInt32LE(size * 2, 8);     // altura (cor + máscara)
  header.writeUInt16LE(1, 12);          // planos
  header.writeUInt16LE(32, 14);         // bpp
  const px = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const s = (y * size + x) * 4, d = ((size - 1 - y) * size + x) * 4; // linhas de baixo para cima
      px[d] = rgba[s + 2]; px[d + 1] = rgba[s + 1]; px[d + 2] = rgba[s]; px[d + 3] = rgba[s + 3];
    }
  }
  const maskRow = Math.ceil(size / 32) * 4;
  return Buffer.concat([header, px, Buffer.alloc(maskRow * size)]);
}

const images = SIZES.map((s) => (s >= 256
  ? new Resvg(svg, { fitTo: { mode: 'width', value: s } }).render().asPng()
  : dib(s)));

const dir = Buffer.alloc(6 + 16 * SIZES.length);
dir.writeUInt16LE(0, 0); dir.writeUInt16LE(1, 2); dir.writeUInt16LE(SIZES.length, 4);
let offset = dir.length;
SIZES.forEach((s, i) => {
  const e = 6 + 16 * i;
  dir[e] = s >= 256 ? 0 : s; dir[e + 1] = s >= 256 ? 0 : s;
  dir.writeUInt16LE(1, e + 4); dir.writeUInt16LE(32, e + 6);
  dir.writeUInt32LE(images[i].length, e + 8); dir.writeUInt32LE(offset, e + 12);
  offset += images[i].length;
});
writeFileSync(out, Buffer.concat([dir, ...images]));
console.log('ico gerado:', out.toString(), SIZES.join(','));
