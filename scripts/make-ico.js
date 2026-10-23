// Buat semua ikon dari SVG: assets/icon.svg + icon.png (256), assets/tray-16/24/32.png, build/icon.ico.
// Ukuran kecil (<= 32 px) memakai versi 4 segmen yang lebih tebal supaya tetap tajam.
// Jalan otomatis sebelum `npm run dist`.
const fs = require('fs');
const path = require('path');
const sharp = require('sharp');

const root = path.join(__dirname, '..');
const ICO_SIZES = [16, 24, 32, 48, 64, 128, 256];

// Tile hitam membulat + roda putih dengan satu segmen menyala (sama seperti UI roda)
function iconSvg(size, small) {
  const C = 128;
  const [R, r, n, gap, lit, dim] = small ? [94, 44, 4, 16, 0, 0.32] : [84, 44, 6, 10, 1, 0.24];
  const pt = (rad, deg) => [C + rad * Math.cos((deg * Math.PI) / 180), C + rad * Math.sin((deg * Math.PI) / 180)];
  const f = (p) => p.map((v) => v.toFixed(2)).join(' ');
  const corner = 5; // sudut segmen dibulatkan lewat stroke tebal berwarna sama
  const segs = [];
  for (let i = 0; i < n; i++) {
    const step = 360 / n;
    const a1 = -90 - step / 2 + i * step + (small ? 45 : 0);
    const a2 = a1 + step;
    const ro = R - corner, ri = r + corner, d = gap / 2 + corner;
    const off = (rad) => (Math.asin(d / rad) * 180) / Math.PI;
    const path =
      `M${f(pt(ro, a1 + off(ro)))}A${ro} ${ro} 0 0 1 ${f(pt(ro, a2 - off(ro)))}` +
      `L${f(pt(ri, a2 - off(ri)))}A${ri} ${ri} 0 0 0 ${f(pt(ri, a1 + off(ri)))}Z`;
    segs.push(`<path d="${path}" fill="#fff" stroke="#fff" stroke-width="${corner * 2}" stroke-linejoin="round" opacity="${i === lit ? 1 : dim}"/>`);
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 256 256">` +
    `<rect x="8" y="8" width="240" height="240" rx="${small ? 52 : 58}" fill="#111"/>` +
    `<rect x="8.5" y="8.5" width="239" height="239" rx="${small ? 51.5 : 57.5}" fill="none" stroke="#fff" stroke-opacity=".08"/>` +
    `${segs.join('')}</svg>`;
}

const png = (size) => sharp(Buffer.from(iconSvg(size, size <= 32))).png().toBuffer();

(async () => {
  fs.writeFileSync(path.join(root, 'assets', 'icon.svg'), iconSvg(256, false));
  fs.writeFileSync(path.join(root, 'assets', 'icon.png'), await png(256));
  for (const s of [16, 24, 32]) fs.writeFileSync(path.join(root, 'assets', `tray-${s}.png`), await png(s));

  // ICO berisi PNG per ukuran (didukung sejak Windows Vista)
  const images = await Promise.all(ICO_SIZES.map(png));
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // tipe: icon
  header.writeUInt16LE(images.length, 4);
  let offset = 6 + 16 * images.length;
  const entries = images.map((img, i) => {
    const e = Buffer.alloc(16);
    e.writeUInt8(ICO_SIZES[i] % 256, 0); // 256 ditulis 0
    e.writeUInt8(ICO_SIZES[i] % 256, 1);
    e.writeUInt16LE(1, 4); // color planes
    e.writeUInt16LE(32, 6); // bit per piksel
    e.writeUInt32LE(img.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += img.length;
    return e;
  });
  const out = path.join(root, 'build', 'icon.ico');
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, Buffer.concat([header, ...entries, ...images]));
  console.log(`Ikon dibuat: icon.svg, icon.png, tray-16/24/32.png, icon.ico (${ICO_SIZES.join(', ')} px)`);
})();
