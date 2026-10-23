// Tes converter.js + tools.js tanpa Electron: node test/converter.test.js
// Tes yang butuh LibreOffice / ffmpeg / pdftoppm dilewati kalau programnya tidak ada.
const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync, execFileSync } = require('child_process');
const sharp = require('sharp');
const { PDFDocument, StandardFonts } = require('pdf-lib');
const { convert, findSoffice, findTool, fodtToText, imagesToPdf } = require('../converter');
const { toolTasks } = require('../tools');

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'shiftwheel-test-'));
const p = (name) => path.join(dir, name);
const has = (name, arg) => findTool(name) !== name || spawnSync(name, [arg], { windowsHide: true }).status === 0;
const hasSoffice = findSoffice() !== 'soffice' || has('soffice', '--version');
const hasFfmpeg = has('ffmpeg', '-version');
const hasPdftoppm = has('pdftoppm', '-v');
const noLo = hasSoffice ? null : 'LibreOffice tidak terinstal';
const noFf = hasFfmpeg ? null : 'ffmpeg tidak terinstal';
const noPoppler = hasPdftoppm ? null : 'pdftoppm tidak terinstal';
const hasOcr = spawnSync('powershell.exe', ['-NoProfile', '-Command',
  '$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]; if ([Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()) { exit 0 } else { exit 1 }'],
{ windowsHide: true }).status === 0;
const noOcr = hasOcr ? null : 'OCR Windows tidak tersedia';
// Gambar berisi tulisan untuk tes OCR
const textSvg = (w, h, lines) => Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><rect width="100%" height="100%" fill="#fff"/>${
  lines.map((t, i) => `<text x="40" y="${90 + i * 80}" font-family="Arial" font-size="44" fill="#000">${t}</text>`).join('')}</svg>`);

let passed = 0, failed = 0, skipped = 0;
async function test(name, fn, skipReason) {
  if (skipReason) { skipped++; return console.log(`SKIP  ${name} (${skipReason})`); }
  try { await fn(); passed++; console.log(`OK    ${name}`); }
  catch (e) { failed++; console.log(`GAGAL ${name}: ${e.message}`); }
}
const rejects = (promise, re) => assert.rejects(promise, (e) => re.test(e.message));
const head = (file, n = 12) => fs.readFileSync(file).subarray(0, n);
const startsWith = (file, bytes) => assert.deepStrictEqual([...head(file, bytes.length)], bytes, `isi awal ${path.basename(file)} salah`);
const ascii = (file, text, offset = 0) => assert.strictEqual(head(file, offset + text.length).subarray(offset).toString('latin1'), text, `${path.basename(file)} bukan ${text}`);
const PDF = [0x25, 0x50, 0x44, 0x46], JPG = [0xff, 0xd8, 0xff], ZIP = [0x50, 0x4b], PNG = [0x89, 0x50, 0x4e, 0x47];
const runTool = async (id, files) => { const outs = []; for (const t of toolTasks(id, files)) outs.push(await t.run()); return outs; };
const pages = async (file) => (await PDFDocument.load(fs.readFileSync(file))).getPageCount();
const ff = (...args) => execFileSync(findTool('ffmpeg'), ['-hide_banner', '-loglevel', 'error', '-y', ...args], { windowsHide: true });

(async () => {
  // ---------- Sampel ----------
  await sharp({ create: { width: 200, height: 100, channels: 4, background: { r: 255, g: 0, b: 0, alpha: 0.5 } } })
    .png().toFile(p('gambar.png'));
  // Gambar "foto" dengan noise supaya kompres bisa diukur
  const noise = Buffer.alloc(800 * 600 * 3);
  for (let i = 0; i < noise.length; i++) noise[i] = (i * 7919) % 251;
  await sharp(noise, { raw: { width: 800, height: 600, channels: 3 } }).jpeg({ quality: 98 }).toFile(p('foto.jpg'));
  fs.writeFileSync(p('catatan.txt'), 'Halo Shiftwheel\nBaris kedua.\n');
  fs.writeFileSync(p('tabel.csv'), 'nama,nilai\nAni,90\nBudi,85\n');

  // PDF dengan urutan gambar teks acak, satu baris terpecah dua, karakter XML, dan 2 halaman
  const doc = await PDFDocument.create();
  const font = await doc.embedFont(StandardFonts.Helvetica);
  const bold = await doc.embedFont(StandardFonts.HelveticaBold);
  const page1 = doc.addPage([595, 842]);
  page1.drawText('Baris kedua: A & B <C>', { x: 50, y: 740, size: 12, font });
  page1.drawText('Halo dari PDF', { x: 50, y: 780, size: 24, font: bold });
  page1.drawText('kanan', { x: 300, y: 700, size: 12, font });
  page1.drawText('kiri', { x: 50, y: 700, size: 12, font: bold });
  doc.addPage([595, 842]).drawText('Halaman dua', { x: 50, y: 780, size: 12, font });
  doc.setAuthor('Rahasia');
  fs.writeFileSync(p('dokumen.pdf'), await doc.save());

  if (hasFfmpeg) {
    ff('-f', 'lavfi', '-i', 'sine=frequency=440:duration=1', p('nada.wav'));
    ff('-f', 'lavfi', '-i', 'testsrc=size=320x240:rate=15:duration=1', '-f', 'lavfi', '-i', 'sine=duration=1',
      '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest', p('klip.mp4'));
    ff('-i', p('gambar.png'), p('lama.bmp'));
  }

  // ---------- Gambar ----------
  await test('png -> jpg (flatten putih, JPEG valid)', async () => {
    const out = await convert(p('gambar.png'), 'jpg');
    assert.strictEqual(out, p('gambar.jpg'));
    startsWith(out, JPG);
    const meta = await sharp(out).metadata();
    assert.strictEqual(meta.width, 200);
    assert.strictEqual(meta.hasAlpha, false);
  });
  await test('png -> pdf (lebar 595 pt, tinggi ikut rasio)', async () => {
    const out = await convert(p('gambar.png'), 'pdf');
    const page = (await PDFDocument.load(fs.readFileSync(out))).getPage(0);
    assert.strictEqual(page.getWidth(), 595);
    assert.strictEqual(Math.round(page.getHeight()), 298);
  });
  for (const [fmt, check] of [['webp', (o) => ascii(o, 'WEBP', 8)], ['avif', (o) => ascii(o, 'ftypavif', 4)], ['tiff', (o) => ascii(o, 'II')], ['gif', (o) => ascii(o, 'GIF8')]]) {
    await test(`png -> ${fmt}`, async () => check(await convert(p('gambar.png'), fmt)));
  }
  await test('png -> bmp (lewat ffmpeg)', async () => ascii(await convert(p('gambar.png'), 'bmp'), 'BM'), noFf);
  await test('bmp -> png (sharp tidak bisa baca BMP, lewat ffmpeg)', async () => {
    const out = await convert(p('lama.bmp'), 'png');
    startsWith(out, PNG);
    assert.strictEqual((await sharp(out).metadata()).width, 200);
  }, noFf);

  await test('gambar -> txt (OCR Windows)', async () => {
    await sharp(textSvg(1000, 220, ['Halo dari Shiftwheel 2026', 'Konversi file &amp; teks: mudah!'])).jpeg().toFile(p('tulisan.jpg'));
    const out = await convert(p('tulisan.jpg'), 'txt');
    assert.strictEqual(out, p('tulisan.txt'));
    assert.strictEqual(fs.readFileSync(out, 'utf8'), 'Halo dari Shiftwheel 2026\nKonversi file & teks: mudah!\n');
  }, noOcr);
  await test('gambar kecil tetap terbaca (diperbesar 2x sebelum OCR)', async () => {
    await sharp(textSvg(1000, 220, ['Tulisan kecil sekali'])).resize(400).png().toFile(p('kecil.png'));
    assert.match(fs.readFileSync(await convert(p('kecil.png'), 'txt'), 'utf8'), /Tulisan kecil sekali/);
  }, noOcr);
  await test('gambar tanpa tulisan -> txt ditolak, tidak ada file kosong', async () => {
    await rejects(convert(p('gambar.png'), 'txt'), /Tidak ada teks yang terbaca di gambar ini/);
    assert.ok(!fs.existsSync(p('gambar.txt')));
  }, noOcr);

  // ---------- Dokumen (LibreOffice) ----------
  await test('txt -> pdf', async () => startsWith(await convert(p('catatan.txt'), 'pdf'), PDF), noLo);
  await test('txt -> docx', async () => startsWith(await convert(p('catatan.txt'), 'docx'), ZIP), noLo);
  await test('docx -> odt', async () => startsWith(await convert(p('catatan.docx'), 'odt'), ZIP), noLo);
  await test('docx -> rtf', async () => ascii(await convert(p('catatan.docx'), 'rtf'), '{\\rtf'), noLo);
  await test('docx -> html', async () => assert.match(fs.readFileSync(await convert(p('catatan.docx'), 'html'), 'utf8'), /Halo Shiftwheel/), noLo);
  await test('csv -> xlsx -> csv (isi tetap)', async () => {
    const xlsx = await convert(p('tabel.csv'), 'xlsx');
    startsWith(xlsx, ZIP);
    fs.renameSync(xlsx, p('tabel2.xlsx'));
    const csv = await convert(p('tabel2.xlsx'), 'csv');
    assert.strictEqual(fs.readFileSync(csv, 'utf8').trim().replace(/\r/g, ''), 'nama,nilai\nAni,90\nBudi,85');
  }, noLo);
  await test('odp -> pptx -> pdf (presentasi)', async () => {
    // Bikin sampel presentasi dari PDF lewat import Impress
    execFileSync(findSoffice(), ['--headless', '--infilter=impress_pdf_import', '--convert-to', 'odp', '--outdir', dir, p('dokumen.pdf')], { windowsHide: true });
    const pptx = await convert(p('dokumen.odp'), 'pptx');
    startsWith(pptx, ZIP);
    startsWith(await convert(pptx, 'pdf'), PDF);
  }, noLo);

  // ---------- PDF ----------
  await test('pdf -> txt (urutan baca, baris digabung, 2 halaman)', async () => {
    const out = await convert(p('dokumen.pdf'), 'txt');
    assert.strictEqual(fs.readFileSync(out, 'utf8'), 'Halo dari PDF\nBaris kedua: A & B <C>\nkiri kanan\n\nHalaman dua\n');
  }, noLo);
  await test('pdf hasil scan -> txt (OCR Windows, 2 halaman)', async () => {
    await sharp(textSvg(900, 200, ['Laporan hasil scan', 'Halaman pertama'])).png().toFile(p('scan1.png'));
    await sharp(textSvg(900, 200, ['Halaman kedua'])).png().toFile(p('scan2.png'));
    await imagesToPdf([p('scan1.png'), p('scan2.png')], p('scan.pdf')); // PDF isi gambar saja
    const text = fs.readFileSync(await convert(p('scan.pdf'), 'txt'), 'utf8');
    assert.match(text, /Laporan hasil scan\nHalaman pertama\n\nHalaman kedua/);
  }, noLo || noPoppler || noOcr);
  await test('pdf tanpa tulisan sama sekali -> pesan error ramah', () =>
    rejects(convert(p('gambar.pdf'), 'txt'), /Tidak ada teks yang terbaca di PDF ini/), noLo || noPoppler || noOcr); // gambar.pdf dari tes png -> pdf
  await test('pdf -> docx', async () => startsWith(await convert(p('dokumen.pdf'), 'docx'), ZIP), noLo);
  await test('pdf -> jpg (2 halaman jadi "dokumen 1.jpg", "dokumen 2.jpg")', async () => {
    assert.strictEqual(await convert(p('dokumen.pdf'), 'jpg'), p('dokumen 1.jpg'));
    startsWith(p('dokumen 1.jpg'), JPG);
    startsWith(p('dokumen 2.jpg'), JPG);
  }, noPoppler);
  await test('pdf 1 halaman -> png (satu file)', async () => startsWith(await convert(p('gambar.pdf'), 'png'), PNG), noPoppler);
  if (!hasSoffice) await test('tanpa LibreOffice -> pesan error ramah', () => rejects(convert(p('catatan.txt'), 'pdf'), /^LibreOffice tidak ditemukan/));
  if (!hasPdftoppm) await test('tanpa pdftoppm -> pesan error ramah', () => rejects(convert(p('dokumen.pdf'), 'jpg'), /pdftoppm tidak ditemukan/));

  // ---------- Audio & video (ffmpeg) ----------
  const audioMagic = { mp3: (o) => assert.ok(/^ID3/.test(head(o, 3).toString('latin1')) || head(o)[0] === 0xff), m4a: (o) => ascii(o, 'ftyp', 4), flac: (o) => ascii(o, 'fLaC'), ogg: (o) => ascii(o, 'OggS'), opus: (o) => ascii(o, 'OggS'), aiff: (o) => ascii(o, 'FORM') };
  for (const [fmt, check] of Object.entries(audioMagic)) {
    await test(`wav -> ${fmt}`, async () => check(await convert(p('nada.wav'), fmt)), noFf);
  }
  await test('mp3 -> wav', async () => ascii(await convert(p('nada.mp3'), 'wav'), 'RIFF'), noFf);
  for (const [fmt, check] of [['webm', (o) => startsWith(o, [0x1a, 0x45, 0xdf, 0xa3])], ['mkv', (o) => startsWith(o, [0x1a, 0x45, 0xdf, 0xa3])], ['mov', (o) => assert.ok(fs.statSync(o).size > 1000)], ['gif', (o) => ascii(o, 'GIF8')], ['mp3', audioMagic.mp3]]) {
    await test(`mp4 -> ${fmt}`, async () => check(await convert(p('klip.mp4'), fmt)), noFf);
  }

  await test('progres video dilaporkan (0..1, untuk HUD)', async () => {
    const seen = [];
    await convert(p('klip.mp4'), 'mkv', { onProgress: (v) => seen.push(v) });
    assert.ok(seen.length > 0, 'tidak ada laporan progres');
    assert.ok(seen.every((v) => v >= 0 && v <= 1), `progres di luar 0..1: ${seen}`);
    assert.ok(Math.max(...seen) > 0.5, `progres tidak sampai ujung: ${seen}`);
  }, noFf);

  // ---------- Tools ----------
  await test('tool gambar: kompres (lebih kecil, maks 2560 px)', async () => {
    const [out] = await runTool('image.compress', [p('foto.jpg')]);
    assert.strictEqual(out, p('foto (dikompres).jpg'));
    assert.ok(fs.statSync(out).size < fs.statSync(p('foto.jpg')).size);
  });
  await test('tool gambar: kompres yang tidak mengecilkan dilewati (hasil dihapus)', async () => {
    // JPEG kualitas 5 pasti membesar kalau di-encode ulang ke kualitas 75
    await sharp({ create: { width: 64, height: 64, channels: 3, background: '#3366cc' } }).jpeg({ quality: 5 }).toFile(p('mini.jpg'));
    await assert.rejects(runTool('image.compress', [p('mini.jpg')]), (e) => e.skipped === true && /Sudah optimal/.test(e.message));
    assert.ok(!fs.existsSync(p('mini (dikompres).jpg')));
  });
  await test('tool gambar: 50%, putar, hitam putih, metadata', async () => {
    const [half] = await runTool('image.half', [p('gambar.png')]);
    assert.deepStrictEqual([(await sharp(half).metadata()).width, (await sharp(half).metadata()).height], [100, 50]);
    const [rot] = await runTool('image.rotate', [p('gambar.png')]);
    assert.deepStrictEqual([(await sharp(rot).metadata()).width, (await sharp(rot).metadata()).height], [100, 200]);
    const [gray] = await runTool('image.gray', [p('foto.jpg')]);
    const { channels } = await sharp(gray).stats();
    assert.ok(Math.abs(channels[0].mean - channels[1].mean) < 1, 'hasil hitam putih masih berwarna');
    const [clean] = await runTool('image.strip', [p('foto.jpg')]);
    assert.strictEqual((await sharp(clean).metadata()).exif, undefined);
  });
  await test('tool gambar: gabung 2 gambar jadi Gabungan.pdf (2 halaman)', async () => {
    const [out] = await runTool('image.merge', [p('gambar.png'), p('foto.jpg')]);
    assert.strictEqual(path.basename(out), 'Gabungan.pdf');
    assert.strictEqual(await pages(out), 2);
  });
  await test('tool PDF: pisah, gabung, putar, hapus metadata', async () => {
    const [first] = await runTool('pdf.split', [p('dokumen.pdf')]);
    assert.strictEqual(first, p('dokumen hal 1.pdf'));
    assert.strictEqual(await pages(p('dokumen hal 2.pdf')), 1);
    const [merged] = await runTool('pdf.merge', [p('dokumen hal 1.pdf'), p('dokumen hal 2.pdf'), p('dokumen.pdf')]);
    assert.strictEqual(path.basename(merged), 'Gabungan (1).pdf'); // Gabungan.pdf sudah dipakai tes gambar
    assert.strictEqual(await pages(merged), 4);
    const [rot] = await runTool('pdf.rotate', [p('dokumen.pdf')]);
    assert.strictEqual((await PDFDocument.load(fs.readFileSync(rot))).getPage(0).getRotation().angle, 90);
    const [clean] = await runTool('pdf.strip', [p('dokumen.pdf')]);
    assert.ok(!(await PDFDocument.load(fs.readFileSync(clean))).getAuthor());
  });
  await test('tool PDF: pisah PDF 1 halaman dilewati', () => assert.rejects(runTool('pdf.split', [p('gambar.pdf')]), (e) => e.skipped));
  await test('tool video: kompres, 720p, bisukan, putar', async () => {
    for (const id of ['compress', '720p', 'mute', 'rotate']) assert.ok(fs.statSync((await runTool(`video.${id}`, [p('klip.mp4')]))[0]).size > 1000, id);
    assert.ok(fs.existsSync(p('klip (tanpa suara).mp4')));
  }, noFf);
  await test('tool audio: kompres, normalisasi, mono', async () => {
    assert.strictEqual((await runTool('audio.compress', [p('nada.wav')]))[0], p('nada (dikompres).mp3'));
    assert.strictEqual((await runTool('audio.normalize', [p('nada.wav')]))[0], p('nada (normal).wav'));
    assert.strictEqual((await runTool('audio.mono', [p('nada.wav')]))[0], p('nada (mono).wav'));
  }, noFf);
  await test('tool salah kategori ditolak', () => rejects(runTool('pdf.rotate', [p('gambar.png')]), /tidak berlaku untuk file \.png/));
  await test('gabung butuh minimal 2 file', () => rejects(runTool('pdf.merge', [p('dokumen.pdf')]), /minimal 2 file/));
  await test('tool tidak dikenal', () => assert.throws(() => toolTasks('image.sulap', [p('gambar.png')]), /Tool tidak dikenal/));

  // ---------- Parser FODT (tanpa LibreOffice) ----------
  await test('fodtToText: urut posisi, entitas, spasi', () => {
    const frame = (pg, x, y, h, body) =>
      `<draw:frame text:anchor-page-number="${pg}" svg:x="${x}" svg:y="${y}" svg:height="${h}"><draw:text-box><text:p>${body}</text:p></draw:text-box></draw:frame>`;
    const xml = [
      frame(2, '1cm', '1cm', '0.5cm', 'hal 2'),
      frame(1, '5in', '1in', '0.2in', 'kanan'),
      frame(1, '1in', '0.5in', '0.3in', 'judul &amp; &lt;x&gt; &#233;'),
      frame(1, '1in', '1in', '0.2in', 'a<text:s text:c="2"/>b'),
    ].join('');
    assert.strictEqual(fodtToText(xml), 'judul & <x> é\na  b kanan\n\nhal 2');
  });

  await test('file input tidak terkunci setelah diproses (bisa dihapus)', async () => {
    // Regresi: cache libvips dulu memegang file GIF yang sudah dikonversi
    await sharp({ create: { width: 50, height: 50, channels: 3, background: '#888' } }).gif().toFile(p('kunci.gif'));
    await convert(p('kunci.gif'), 'png');
    await runTool('image.rotate', [p('kunci.gif')]);
    fs.rmSync(p('kunci.gif'));
    fs.rmSync(p('foto.jpg')); // sudah dibaca banyak tools di atas
  });

  // ---------- Kasus error ----------
  await test('format sama dilewati', () =>
    assert.rejects(convert(p('catatan.txt'), 'txt'), (e) => e.skipped === true && e.message === 'Sudah berformat TXT'));
  await test('jpeg dianggap jpg (dilewati)', async () => {
    fs.copyFileSync(p('gambar.jpg'), p('salinan.jpeg'));
    await assert.rejects(convert(p('salinan.jpeg'), 'jpg'), (e) => e.skipped);
  });
  await test('gambar -> docx ditolak', () => rejects(convert(p('gambar.png'), 'docx'), /Gambar tidak bisa dikonversi ke DOCX/));
  await test('dokumen -> jpg ditolak', () => rejects(convert(p('catatan.txt'), 'jpg'), /Dokumen tidak bisa dikonversi ke JPG/));
  await test('audio -> pdf ditolak', () => rejects(convert(p('nada.wav'), 'pdf'), /Audio tidak bisa dikonversi ke PDF/), noFf);
  await test('tipe tidak didukung', async () => {
    fs.writeFileSync(p('arsip.zip'), 'x');
    await rejects(convert(p('arsip.zip'), 'pdf'), /Tipe file \.zip tidak didukung/);
  });
  await test('folder ditolak', () => rejects(convert(dir, 'pdf'), /Folder tidak bisa diproses/));
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
