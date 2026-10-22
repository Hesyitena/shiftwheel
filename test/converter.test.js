// Tes converter.js tanpa Electron: node test/converter.test.js
const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync } = require('child_process');
const sharp = require('sharp');
const { PDFDocument, StandardFonts } = require('pdf-lib');
const { convert, findSoffice } = require('../converter');

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'shiftwheel-test-'));
const p = (name) => path.join(dir, name);
const hasCmd = (cmd, arg) => spawnSync(cmd, [arg], { windowsHide: true }).status === 0;
const hasSoffice = findSoffice() !== 'soffice' || hasCmd('soffice', '--version');
const hasPdftoppm = hasCmd('pdftoppm', '-v');

let passed = 0, failed = 0, skipped = 0;
async function test(name, fn, skipReason) {
  if (skipReason) { skipped++; return console.log(`SKIP  ${name} (${skipReason})`); }
  try { await fn(); passed++; console.log(`OK    ${name}`); }
  catch (e) { failed++; console.log(`GAGAL ${name}: ${e.message}`); }
}
const rejects = (promise, re) => assert.rejects(promise, (e) => re.test(e.message));
const magic = (file, bytes) => assert.deepStrictEqual([...fs.readFileSync(file).subarray(0, bytes.length)], bytes);
const PDF = [0x25, 0x50, 0x44, 0x46], JPG = [0xff, 0xd8, 0xff], ZIP = [0x50, 0x4b];

(async () => {
  // File sampel
  await sharp({ create: { width: 200, height: 100, channels: 4, background: { r: 255, g: 0, b: 0, alpha: 0.5 } } })
    .png().toFile(p('gambar.png'));
  fs.writeFileSync(p('catatan.txt'), 'Halo Shiftwheel\nBaris kedua.\n');
  const doc = await PDFDocument.create();
  const font = await doc.embedFont(StandardFonts.Helvetica);
  doc.addPage([595, 842]).drawText('Halo dari PDF', { x: 50, y: 780, size: 24, font });
  fs.writeFileSync(p('dokumen.pdf'), await doc.save());

  await test('png -> jpg (flatten putih, JPEG valid)', async () => {
    const out = await convert(p('gambar.png'), 'jpg');
    assert.strictEqual(out, p('gambar.jpg'));
    magic(out, JPG);
    const meta = await sharp(out).metadata();
    assert.strictEqual(meta.width, 200);
    assert.strictEqual(meta.hasAlpha, false);
  });

  await test('png -> pdf (lebar 595 pt, tinggi ikut rasio)', async () => {
    const out = await convert(p('gambar.png'), 'pdf');
    magic(out, PDF);
    const page = (await PDFDocument.load(fs.readFileSync(out))).getPage(0);
    assert.strictEqual(page.getWidth(), 595);
    assert.strictEqual(Math.round(page.getHeight()), 298);
  });

  const noLo = hasSoffice ? null : 'LibreOffice tidak terinstal';
  await test('txt -> pdf', async () => magic(await convert(p('catatan.txt'), 'pdf'), PDF), noLo);
  await test('txt -> docx', async () => magic(await convert(p('catatan.txt'), 'docx'), ZIP), noLo);
  await test('pdf -> txt', async () => {
    const out = await convert(p('dokumen.pdf'), 'txt');
    assert.match(fs.readFileSync(out, 'utf8'), /Halo dari PDF/);
  }, noLo);
  await test('pdf -> docx', async () => magic(await convert(p('dokumen.pdf'), 'docx'), ZIP), noLo);
  if (!hasSoffice) {
    await test('tanpa LibreOffice -> pesan error ramah', () =>
      rejects(convert(p('catatan.txt'), 'pdf'), /^LibreOffice tidak ditemukan\. Install LibreOffice dulu\.$/));
  }

  await test('pdf -> jpg (pdftoppm)', async () => magic(await convert(p('dokumen.pdf'), 'jpg'), JPG),
    hasPdftoppm ? null : 'pdftoppm tidak terinstal');
  if (!hasPdftoppm) {
    await test('tanpa pdftoppm -> pesan error ramah', () => rejects(convert(p('dokumen.pdf'), 'jpg'), /pdftoppm tidak ditemukan/));
  }

  // Kasus error
  await test('format sama dilewati', () =>
    assert.rejects(convert(p('catatan.txt'), 'txt'), (e) => e.skipped === true && e.message === 'Sudah berformat TXT'));
  await test('jpeg dianggap jpg (dilewati)', async () => {
    fs.copyFileSync(p('gambar.jpg'), p('foto.jpeg'));
    await assert.rejects(convert(p('foto.jpeg'), 'jpg'), (e) => e.skipped);
  });
  await test('gambar -> docx ditolak', () => rejects(convert(p('gambar.png'), 'docx'), /Gambar tidak bisa dikonversi ke DOCX/));
  await test('dokumen -> jpg ditolak', () => rejects(convert(p('catatan.txt'), 'jpg'), /Dokumen tidak bisa dikonversi langsung ke JPG/));
  await test('tipe tidak didukung', async () => {
    fs.writeFileSync(p('arsip.zip'), 'x');
    await rejects(convert(p('arsip.zip'), 'pdf'), /Tipe file \.zip tidak didukung/);
  });
  await test('file tidak ada', () => rejects(convert(p('hilang.png'), 'jpg'), /File tidak ditemukan/));
  await test('format tujuan tidak dikenal', () => rejects(convert(p('gambar.png'), 'exe'), /Format tujuan tidak dikenal/));
  await test('nama bentrok -> (1), (2)', async () => {
    // gambar.jpg sudah ada dari tes pertama
    assert.strictEqual(await convert(p('gambar.png'), 'jpg'), p('gambar (1).jpg'));
    assert.strictEqual(await convert(p('gambar.png'), 'jpg'), p('gambar (2).jpg'));
    assert.ok(fs.existsSync(p('gambar.jpg')), 'file lama tidak boleh tertimpa');
  });

  fs.rmSync(dir, { recursive: true, force: true });
  console.log(`\n${passed} lulus, ${failed} gagal, ${skipped} dilewati`);
  process.exit(failed ? 1 : 0);
})();
