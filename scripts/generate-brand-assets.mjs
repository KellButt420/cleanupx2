// Build-only SVG rasterizer. No Node dependency is shipped with Cleanup.
// npm install --prefix artifacts/asset-tools --no-save --package-lock=false @resvg/resvg-js@2.6.2
// node scripts/generate-brand-assets.mjs
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const require = createRequire(join(root, 'artifacts/asset-tools/package.json'));
const { Resvg } = require('@resvg/resvg-js');
const assets = join(root, 'src/frontend/Assets');
const source = readFileSync(join(assets, 'LogoViolet.svg'), 'utf8');
const paths = source.replace(/^[\s\S]*?<svg\b[^>]*>/, '').replace(/<\/svg>\s*$/, '');

function render(width, height, scale = 0.82) {
  const size = Math.min(width, height) * scale;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
  <svg x="${(width-size)/2}" y="${(height-size)/2}" width="${size}" height="${size}" viewBox="0 0 638 638">${paths}</svg></svg>`;
  return Buffer.from(new Resvg(svg).render().asPng());
}

for (const [name, width, height] of [
  ['Square44x44Logo',44,44], ['Square150x150Logo',150,150],
  ['Wide310x150Logo',310,150], ['StoreLogo',50,50],
]) {
  writeFileSync(join(assets, `${name}.png`), render(width,height));
  for (const scale of [125,150,200,400]) {
    writeFileSync(join(assets, `${name}.scale-${scale}.png`), render(Math.round(width*scale/100),Math.round(height*scale/100)));
  }
}
const iconSizes = [16,24,32,48,64,256];
const images = iconSizes.map(size => render(size,size));
for (let i=0; i<iconSizes.length; i++) {
  writeFileSync(join(assets, `Square44x44Logo.targetsize-${iconSizes[i]}_altform-unplated.png`),images[i]);
}
const header = Buffer.alloc(6+16*images.length);
header.writeUInt16LE(1,2); header.writeUInt16LE(images.length,4);
let offset=header.length;
for (let i=0; i<images.length; i++) {
  const p=6+16*i, size=iconSizes[i];
  header[p]=size===256?0:size; header[p+1]=header[p];
  header.writeUInt16LE(1,p+4); header.writeUInt16LE(32,p+6);
  header.writeUInt32LE(images[i].length,p+8); header.writeUInt32LE(offset,p+12);
  offset+=images[i].length;
}
writeFileSync(join(assets,'Cleanup.ico'),Buffer.concat([header,...images]));
console.log('Generated MSIX logo scales, shell target sizes and multi-resolution Cleanup.ico from LogoViolet.svg.');
