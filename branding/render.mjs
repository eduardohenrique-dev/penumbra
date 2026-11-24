// Gera os PNGs do logo a partir de branding/logo.svg (usa @resvg/resvg-js).
import { Resvg } from '@resvg/resvg-js';
import { readFileSync, writeFileSync } from 'node:fs';

const svg = readFileSync(new URL('./logo.svg', import.meta.url), 'utf8');
const sizes = { 'app-icon.png': 1024 };
for (const [name, size] of Object.entries(sizes)) {
  const png = new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().asPng();
  writeFileSync(new URL(name === 'app-icon.png' ? '../app-icon.png' : './' + name, import.meta.url), png);
}
console.log('ok');
