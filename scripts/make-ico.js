// Buat build/icon.ico (16, 32, 48, 256) dari assets/icon.png. Jalan otomatis sebelum `npm run dist`.
const fs = require('fs');
const path = require('path');
const sharp = require('sharp');

const root = path.join(__dirname, '..');
const SIZES = [16, 32, 48, 256];

(async () => {
  // Tiap ukuran disimpan sebagai PNG di dalam ICO (didukung sejak Windows Vista)
  const images = await Promise.all(
    SIZES.map((s) => sharp(path.join(root, 'assets', 'icon.png')).resize(s, s, { kernel: 'lanczos3' }).png().toBuffer())
  );

  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // tipe: icon
  header.writeUInt16LE(images.length, 4);

  let offset = 6 + 16 * images.length;
  const entries = images.map((img, i) => {
    const e = Buffer.alloc(16);
    e.writeUInt8(SIZES[i] % 256, 0); // 256 ditulis 0
    e.writeUInt8(SIZES[i] % 256, 1);
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
  console.log(`icon.ico dibuat (${SIZES.join(', ')} px)`);
})();
