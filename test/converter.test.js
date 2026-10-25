// Tests for converter.js + tools.js without Electron: node test/converter.test.js
// Tests that need LibreOffice / ffmpeg / pdftoppm / Windows OCR are skipped when the program isn't there.
const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawnSync, execFileSync } = require('child_process');
const sharp = require('sharp');
const { PDFDocument, StandardFonts } = require('pdf-lib');
const { convert, findSoffice, findTool, fodtToText, imagesToPdf, probeDuration } = require('../converter');
const { toolTasks, previewImage, cropImage } = require('../tools');

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'shiftwheel-test-'));
const p = (name) => path.join(dir, name);
const has = (name, arg) => findTool(name) !== name || spawnSync(name, [arg], { windowsHide: true }).status === 0;
const hasSoffice = findSoffice() !== 'soffice' || has('soffice', '--version');
const hasFfmpeg = has('ffmpeg', '-version');
const hasPdftoppm = has('pdftoppm', '-v');
const noLo = hasSoffice ? null : 'LibreOffice not installed';
const noFf = hasFfmpeg ? null : 'ffmpeg not installed';
const noPoppler = hasPdftoppm ? null : 'pdftoppm not installed';
const hasOcr = spawnSync('powershell.exe', ['-NoProfile', '-Command',
  '$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]; if ([Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()) { exit 0 } else { exit 1 }'],
{ windowsHide: true }).status === 0;
const noOcr = hasOcr ? null : 'Windows OCR not available';
// Images with writing for the OCR tests
const textSvg = (w, h, lines) => Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><rect width="100%" height="100%" fill="#fff"/>${
  lines.map((t, i) => `<text x="40" y="${90 + i * 80}" font-family="Arial" font-size="44" fill="#000">${t}</text>`).join('')}</svg>`);

let passed = 0, failed = 0, skipped = 0;
async function test(name, fn, skipReason) {
  if (skipReason) { skipped++; return console.log(`SKIP  ${name} (${skipReason})`); }
  try { await fn(); passed++; console.log(`OK    ${name}`); }
  catch (e) { failed++; console.log(`FAIL  ${name}: ${e.message}`); }
}
const rejects = (promise, re) => assert.rejects(promise, (e) => re.test(e.message));
const head = (file, n = 12) => fs.readFileSync(file).subarray(0, n);
const startsWith = (file, bytes) => assert.deepStrictEqual([...head(file, bytes.length)], bytes, `${path.basename(file)} starts with the wrong bytes`);
const ascii = (file, text, offset = 0) => assert.strictEqual(head(file, offset + text.length).subarray(offset).toString('latin1'), text, `${path.basename(file)} is not ${text}`);
const PDF = [0x25, 0x50, 0x44, 0x46], JPG = [0xff, 0xd8, 0xff], ZIP = [0x50, 0x4b], PNG = [0x89, 0x50, 0x4e, 0x47];
const runTool = async (id, files) => { const outs = []; for (const t of toolTasks(id, files)) outs.push(await t.run()); return outs; };
const pages = async (file) => (await PDFDocument.load(fs.readFileSync(file))).getPageCount();
const ff = (...args) => execFileSync(findTool('ffmpeg'), ['-hide_banner', '-loglevel', 'error', '-y', ...args], { windowsHide: true });

(async () => {
  // ---------- Samples ----------
  await sharp({ create: { width: 200, height: 100, channels: 4, background: { r: 255, g: 0, b: 0, alpha: 0.5 } } })
    .png().toFile(p('picture.png'));
  // A noisy "photo" so compression can be measured
  const noise = Buffer.alloc(800 * 600 * 3);
  for (let i = 0; i < noise.length; i++) noise[i] = (i * 7919) % 251;
  await sharp(noise, { raw: { width: 800, height: 600, channels: 3 } }).jpeg({ quality: 98 }).toFile(p('photo.jpg'));
  fs.writeFileSync(p('notes.txt'), 'Hello Shiftwheel\nSecond line.\n');
  fs.writeFileSync(p('table.csv'), 'name,score\nAnn,90\nBob,85\n');

  // A PDF with text drawn out of order, one line split in two, XML characters, and 2 pages
  const doc = await PDFDocument.create();
  const font = await doc.embedFont(StandardFonts.Helvetica);
  const bold = await doc.embedFont(StandardFonts.HelveticaBold);
  const page1 = doc.addPage([595, 842]);
  page1.drawText('Second line: A & B <C>', { x: 50, y: 740, size: 12, font });
  page1.drawText('Hello from PDF', { x: 50, y: 780, size: 24, font: bold });
  page1.drawText('right', { x: 300, y: 700, size: 12, font });
  page1.drawText('left', { x: 50, y: 700, size: 12, font: bold });
  doc.addPage([595, 842]).drawText('Page two', { x: 50, y: 780, size: 12, font });
  doc.setAuthor('Secret');
  fs.writeFileSync(p('document.pdf'), await doc.save());

  if (hasFfmpeg) {
    ff('-f', 'lavfi', '-i', 'sine=frequency=440:duration=1', p('tone.wav'));
    ff('-f', 'lavfi', '-i', 'testsrc=size=320x240:rate=15:duration=1', '-f', 'lavfi', '-i', 'sine=duration=1',
      '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest', p('clip.mp4'));
    ff('-i', p('picture.png'), p('old.bmp'));
  }

  // ---------- Images ----------
  await test('png -> jpg (flattened onto white, valid JPEG)', async () => {
    const out = await convert(p('picture.png'), 'jpg');
    assert.strictEqual(out, p('picture.jpg'));
    startsWith(out, JPG);
    const meta = await sharp(out).metadata();
    assert.strictEqual(meta.width, 200);
    assert.strictEqual(meta.hasAlpha, false);
  });
  await test('png -> pdf (595 pt wide, height follows aspect ratio)', async () => {
    const out = await convert(p('picture.png'), 'pdf');
    const page = (await PDFDocument.load(fs.readFileSync(out))).getPage(0);
    assert.strictEqual(page.getWidth(), 595);
    assert.strictEqual(Math.round(page.getHeight()), 298);
  });
  for (const [fmt, check] of [['webp', (o) => ascii(o, 'WEBP', 8)], ['avif', (o) => ascii(o, 'ftypavif', 4)], ['tiff', (o) => ascii(o, 'II')], ['gif', (o) => ascii(o, 'GIF8')]]) {
    await test(`png -> ${fmt}`, async () => check(await convert(p('picture.png'), fmt)));
  }
  await test('png -> bmp (through ffmpeg)', async () => ascii(await convert(p('picture.png'), 'bmp'), 'BM'), noFf);
  await test("bmp -> png (sharp can't read BMP, through ffmpeg)", async () => {
    const out = await convert(p('old.bmp'), 'png');
    startsWith(out, PNG);
    assert.strictEqual((await sharp(out).metadata()).width, 200);
  }, noFf);

  await test('image -> txt (Windows OCR)', async () => {
    await sharp(textSvg(1000, 220, ['Hello from Shiftwheel 2026', 'Convert files &amp; text: easy!'])).jpeg().toFile(p('writing.jpg'));
    const out = await convert(p('writing.jpg'), 'txt');
    assert.strictEqual(out, p('writing.txt'));
    assert.strictEqual(fs.readFileSync(out, 'utf8'), 'Hello from Shiftwheel 2026\nConvert files & text: easy!\n');
  }, noOcr);
  await test('small images still read (upscaled 2x before OCR)', async () => {
    await sharp(textSvg(1000, 220, ['Very small writing'])).resize(400).png().toFile(p('small.png'));
    assert.match(fs.readFileSync(await convert(p('small.png'), 'txt'), 'utf8'), /Very small writing/);
  }, noOcr);
  await test('image without writing -> txt is rejected, no empty file', async () => {
    await rejects(convert(p('picture.png'), 'txt'), /No readable text found in this image/);
    assert.ok(!fs.existsSync(p('picture.txt')));
  }, noOcr);

  // ---------- Documents (LibreOffice) ----------
  await test('txt -> pdf', async () => startsWith(await convert(p('notes.txt'), 'pdf'), PDF), noLo);
  await test('txt -> docx', async () => startsWith(await convert(p('notes.txt'), 'docx'), ZIP), noLo);
  await test('docx -> odt', async () => startsWith(await convert(p('notes.docx'), 'odt'), ZIP), noLo);
  await test('docx -> rtf', async () => ascii(await convert(p('notes.docx'), 'rtf'), '{\\rtf'), noLo);
  await test('docx -> html', async () => assert.match(fs.readFileSync(await convert(p('notes.docx'), 'html'), 'utf8'), /Hello Shiftwheel/), noLo);
  await test('md -> html, pdf, docx, txt (formatted Markdown)', async () => {
    fs.writeFileSync(p('readme.md'), '# Heading 1\n\nParagraph with **bold** and `code`.\n\n| A | B |\n|---|---|\n| 1 | 2 |\n');
    const html = await convert(p('readme.md'), 'html');
    assert.match(fs.readFileSync(html, 'utf8'), /<h1>Heading 1<\/h1>[\s\S]*<strong>bold<\/strong>[\s\S]*<table>/);
    if (!noLo) {
      startsWith(await convert(p('readme.md'), 'pdf'), PDF);
      startsWith(await convert(p('readme.md'), 'docx'), ZIP);
      assert.match(fs.readFileSync(await convert(p('readme.md'), 'txt'), 'utf8'), /Heading 1/);
    }
  });
  await test('csv -> xlsx -> csv (content unchanged)', async () => {
    const xlsx = await convert(p('table.csv'), 'xlsx');
    startsWith(xlsx, ZIP);
    fs.renameSync(xlsx, p('table2.xlsx'));
    const csv = await convert(p('table2.xlsx'), 'csv');
    assert.strictEqual(fs.readFileSync(csv, 'utf8').trim().replace(/\r/g, ''), 'name,score\nAnn,90\nBob,85');
  }, noLo);
  await test('odp -> pptx -> pdf (presentations)', async () => {
    // Make a sample presentation from a PDF through the Impress importer
    execFileSync(findSoffice(), ['--headless', '--infilter=impress_pdf_import', '--convert-to', 'odp', '--outdir', dir, p('document.pdf')], { windowsHide: true });
    const pptx = await convert(p('document.odp'), 'pptx');
    startsWith(pptx, ZIP);
    startsWith(await convert(pptx, 'pdf'), PDF);
  }, noLo);

  // ---------- PDF ----------
  await test('pdf -> txt (reading order, merged lines, 2 pages)', async () => {
    const out = await convert(p('document.pdf'), 'txt');
    assert.strictEqual(fs.readFileSync(out, 'utf8'), 'Hello from PDF\nSecond line: A & B <C>\nleft right\n\nPage two\n');
  }, noLo);
  await test('scanned pdf -> txt (Windows OCR, 2 pages)', async () => {
    await sharp(textSvg(900, 200, ['Scanned report', 'First page'])).png().toFile(p('scan1.png'));
    await sharp(textSvg(900, 200, ['Second page'])).png().toFile(p('scan2.png'));
    await imagesToPdf([p('scan1.png'), p('scan2.png')], p('scan.pdf')); // a PDF of images only
    const text = fs.readFileSync(await convert(p('scan.pdf'), 'txt'), 'utf8');
    assert.match(text, /Scanned report\nFirst page\n\nSecond page/);
  }, noLo || noPoppler || noOcr);
  await test('pdf with no writing at all -> friendly error', () =>
    rejects(convert(p('picture.pdf'), 'txt'), /No readable text found in this PDF/), noLo || noPoppler || noOcr); // picture.pdf comes from the png -> pdf test
  await test('pdf -> docx', async () => startsWith(await convert(p('document.pdf'), 'docx'), ZIP), noLo);
  await test('pdf -> jpg (2 pages become "document 1.jpg", "document 2.jpg")', async () => {
    assert.strictEqual(await convert(p('document.pdf'), 'jpg'), p('document 1.jpg'));
    startsWith(p('document 1.jpg'), JPG);
    startsWith(p('document 2.jpg'), JPG);
  }, noPoppler);
  await test('1-page pdf -> png (a single file)', async () => startsWith(await convert(p('picture.pdf'), 'png'), PNG), noPoppler);
  if (!hasSoffice) await test('without LibreOffice -> friendly error', () => rejects(convert(p('notes.txt'), 'pdf'), /^LibreOffice not found/));
  if (!hasPdftoppm) await test('without pdftoppm -> friendly error', () => rejects(convert(p('document.pdf'), 'jpg'), /pdftoppm not found/));

  // ---------- Audio & video (ffmpeg) ----------
  const audioMagic = { mp3: (o) => assert.ok(/^ID3/.test(head(o, 3).toString('latin1')) || head(o)[0] === 0xff), m4a: (o) => ascii(o, 'ftyp', 4), flac: (o) => ascii(o, 'fLaC'), ogg: (o) => ascii(o, 'OggS'), opus: (o) => ascii(o, 'OggS'), aiff: (o) => ascii(o, 'FORM') };
  for (const [fmt, check] of Object.entries(audioMagic)) {
    await test(`wav -> ${fmt}`, async () => check(await convert(p('tone.wav'), fmt)), noFf);
  }
  await test('mp3 -> wav', async () => ascii(await convert(p('tone.mp3'), 'wav'), 'RIFF'), noFf);
  for (const [fmt, check] of [['webm', (o) => startsWith(o, [0x1a, 0x45, 0xdf, 0xa3])], ['mkv', (o) => startsWith(o, [0x1a, 0x45, 0xdf, 0xa3])], ['mov', (o) => assert.ok(fs.statSync(o).size > 1000)], ['gif', (o) => ascii(o, 'GIF8')], ['mp3', audioMagic.mp3]]) {
    await test(`mp4 -> ${fmt}`, async () => check(await convert(p('clip.mp4'), fmt)), noFf);
  }

  await test('video progress is reported (0..1, for the HUD)', async () => {
    const seen = [];
    await convert(p('clip.mp4'), 'mkv', { onProgress: (v) => seen.push(v) });
    assert.ok(seen.length > 0, 'no progress reported');
    assert.ok(seen.every((v) => v >= 0 && v <= 1), `progress outside 0..1: ${seen}`);
    assert.ok(Math.max(...seen) > 0.5, `progress never got near the end: ${seen}`);
  }, noFf);

  // ---------- Tools ----------
  await test('image tool: compress (smaller, max 2560 px)', async () => {
    const [out] = await runTool('image.compress', [p('photo.jpg')]);
    assert.strictEqual(out, p('photo (compressed).jpg'));
    assert.ok(fs.statSync(out).size < fs.statSync(p('photo.jpg')).size);
  });
  await test("image tool: compress that doesn't shrink is skipped (output removed)", async () => {
    // A quality-5 JPEG always grows when re-encoded at quality 75
    await sharp({ create: { width: 64, height: 64, channels: 3, background: '#3366cc' } }).jpeg({ quality: 5 }).toFile(p('tiny.jpg'));
    await assert.rejects(runTool('image.compress', [p('tiny.jpg')]), (e) => e.skipped === true && /Already optimized/.test(e.message));
    assert.ok(!fs.existsSync(p('tiny (compressed).jpg')));
  });
  await test('image tools: 50%, rotate, grayscale, metadata', async () => {
    const [half] = await runTool('image.half', [p('picture.png')]);
    assert.deepStrictEqual([(await sharp(half).metadata()).width, (await sharp(half).metadata()).height], [100, 50]);
    const [rot] = await runTool('image.rotate', [p('picture.png')]);
    assert.deepStrictEqual([(await sharp(rot).metadata()).width, (await sharp(rot).metadata()).height], [100, 200]);
    const [gray] = await runTool('image.gray', [p('photo.jpg')]);
    const { channels } = await sharp(gray).stats();
    assert.ok(Math.abs(channels[0].mean - channels[1].mean) < 1, 'the grayscale result still has color');
    const [clean] = await runTool('image.strip', [p('photo.jpg')]);
    assert.strictEqual((await sharp(clean).metadata()).exif, undefined);
  });
  await test('image tool: merge 2 images into Merged.pdf (2 pages)', async () => {
    const [out] = await runTool('image.merge', [p('picture.png'), p('photo.jpg')]);
    assert.strictEqual(path.basename(out), 'Merged.pdf');
    assert.strictEqual(await pages(out), 2);
  });
  await test('PDF tools: split, merge, rotate, remove metadata', async () => {
    const [first] = await runTool('pdf.split', [p('document.pdf')]);
    assert.strictEqual(first, p('document page 1.pdf'));
    assert.strictEqual(await pages(p('document page 2.pdf')), 1);
    const [merged] = await runTool('pdf.merge', [p('document page 1.pdf'), p('document page 2.pdf'), p('document.pdf')]);
    assert.strictEqual(path.basename(merged), 'Merged (1).pdf'); // Merged.pdf was taken by the image test
    assert.strictEqual(await pages(merged), 4);
    const [rot] = await runTool('pdf.rotate', [p('document.pdf')]);
    assert.strictEqual((await PDFDocument.load(fs.readFileSync(rot))).getPage(0).getRotation().angle, 90);
    const [clean] = await runTool('pdf.strip', [p('document.pdf')]);
    assert.ok(!(await PDFDocument.load(fs.readFileSync(clean))).getAuthor());
  });
  await test('PDF tool: splitting a 1-page PDF is skipped', () => assert.rejects(runTool('pdf.split', [p('picture.pdf')]), (e) => e.skipped));
  await test('video tools: compress, 720p, mute, rotate', async () => {
    for (const id of ['compress', '720p', 'mute', 'rotate']) assert.ok(fs.statSync((await runTool(`video.${id}`, [p('clip.mp4')]))[0]).size > 1000, id);
    assert.ok(fs.existsSync(p('clip (muted).mp4')));
  }, noFf);
  await test('audio tools: compress, normalize, mono', async () => {
    assert.strictEqual((await runTool('audio.compress', [p('tone.wav')]))[0], p('tone (compressed).mp3'));
    assert.strictEqual((await runTool('audio.normalize', [p('tone.wav')]))[0], p('tone (normalized).wav'));
    assert.strictEqual((await runTool('audio.mono', [p('tone.wav')]))[0], p('tone (mono).wav'));
  }, noFf);

  const dims = async (f) => { const m = await sharp(f).metadata(); return [m.width, m.height]; };
  const near = (actual, expected, tol, what) => assert.ok(Math.abs(actual - expected) <= tol, `${what}: ${actual} (expected ~${expected})`);

  await test('image tools: mirror, trim edges, square', async () => {
    const [flip] = await runTool('image.flip', [p('picture.png')]);
    assert.deepStrictEqual(await dims(flip), [200, 100]);
    // A 50x50 black square in the middle of a 200x200 white canvas -> trimmed to 50x50
    await sharp({ create: { width: 200, height: 200, channels: 3, background: '#fff' } })
      .composite([{ input: { create: { width: 50, height: 50, channels: 3, background: '#000' } }, left: 75, top: 75 }]).png().toFile(p('framed.png'));
    assert.deepStrictEqual(await dims((await runTool('image.trim', [p('framed.png')]))[0]), [50, 50]);
    assert.deepStrictEqual(await dims((await runTool('image.square', [p('photo.jpg')]))[0]), [600, 600]);
  });
  await test('PDF tools: page numbers and reverse', async () => {
    const two = await PDFDocument.create();
    two.addPage([300, 300]);
    two.addPage([600, 400]);
    fs.writeFileSync(p('sizes.pdf'), await two.save());
    const [rev] = await runTool('pdf.reverse', [p('sizes.pdf')]);
    assert.strictEqual((await PDFDocument.load(fs.readFileSync(rev))).getPage(0).getWidth(), 600);
    const [num] = await runTool('pdf.number', [p('sizes.pdf')]);
    const text = execFileSync(findTool('pdftotext'), [num, '-'], { windowsHide: true }).toString();
    assert.match(text, /1\s*\/\s*2[\s\S]*2\s*\/\s*2/); // pdftotext may drop thin spaces: "1/2"
  }, noPoppler);
  await test('video tools: thumbnail and 2× speed', async () => {
    const [thumb] = await runTool('video.thumb', [p('clip.mp4')]);
    startsWith(thumb, JPG);
    const [fast] = await runTool('video.fast', [p('clip.mp4')]);
    near(await probeDuration(fast), 0.5, 0.15, 'duration at 2× speed');
  }, noFf);
  await test('audio tools: denoise, trim silence, 1.5× speed, fade', async () => {
    ff('-f', 'lavfi', '-i', 'sine=frequency=440:duration=1', '-af', 'adelay=1000,apad=pad_dur=1', p('gap.wav')); // 1 s silence + 1 s tone + 1 s silence
    near(await probeDuration((await runTool('audio.trim', [p('gap.wav')]))[0]), 1, 0.2, 'duration after trimming silence');
    near(await probeDuration((await runTool('audio.fast', [p('tone.wav')]))[0]), 1 / 1.5, 0.1, 'duration at 1.5× speed');
    assert.ok(fs.statSync((await runTool('audio.denoise', [p('tone.wav')]))[0]).size > 1000);
    assert.ok(fs.statSync((await runTool('audio.fade', [p('tone.wav')]))[0]).size > 1000);
  }, noFf);
  await test('document tools: merge into PDF, image per page/slide', async () => {
    const [merged] = await runTool('document.merge', [p('notes.txt'), p('notes.docx')]);
    assert.ok(path.basename(merged).startsWith('Merged'));
    assert.strictEqual(await pages(merged), 2);
    const [single] = await runTool('document.pages', [p('notes.txt')]);
    assert.strictEqual(single, p('notes (page).png'));
    startsWith(single, PNG);
    const [slide1] = await runTool('slides.pages', [p('document.odp')]); // 2 slides from the presentation test
    assert.strictEqual(path.extname(slide1), '.png');
    assert.ok(fs.existsSync(slide1.replace(/1\.png$/, '2.png')), 'the second slide is missing');
  }, noLo || noPoppler);

  await test('crop: normalized rect cuts the right pixels', async () => {
    const out = await cropImage(p('picture.png'), { x: 0.25, y: 0, w: 0.5, h: 1 });
    assert.strictEqual(out, p('picture (cropped).png'));
    assert.deepStrictEqual(await dims(out), [100, 100]);
  });
  await test('crop: preview is a downscaled JPEG data URL with the full size', async () => {
    const pv = await previewImage(p('framed.png'));
    assert.deepStrictEqual([pv.width, pv.height], [200, 200]);
    assert.ok(pv.src.startsWith('data:image/jpeg;base64,'));
  });
  await test('crop: the tool needs the editor, it never runs straight from the queue', () =>
    assert.throws(() => toolTasks('image.crop', [p('picture.png')]), /opens an editor/));
  await test("trim edges on a photo with no plain border is skipped (no identical copy)", async () => {
    const noise = Buffer.alloc(300 * 200 * 3);
    for (let i = 0; i < noise.length; i++) noise[i] = (i * 7919) % 251;
    await sharp(noise, { raw: { width: 300, height: 200, channels: 3 } }).jpeg().toFile(p('busy.jpg'));
    await assert.rejects(runTool('image.trim', [p('busy.jpg')]), (e) => e.skipped && /No plain border/.test(e.message));
    assert.ok(!fs.existsSync(p('busy (trimmed).jpg')));
  });

  await test('tool on the wrong category is rejected', () => rejects(runTool('pdf.rotate', [p('picture.png')]), /doesn't work on \.png files/));
  await test('merging needs at least 2 files', () => rejects(runTool('pdf.merge', [p('document.pdf')]), /at least 2 files/));
  await test('unknown tool', () => assert.throws(() => toolTasks('image.magic', [p('picture.png')]), /Unknown tool/));

  // ---------- FODT parser (no LibreOffice) ----------
  await test('fodtToText: position order, entities, spaces', () => {
    const frame = (pg, x, y, h, body) =>
      `<draw:frame text:anchor-page-number="${pg}" svg:x="${x}" svg:y="${y}" svg:height="${h}"><draw:text-box><text:p>${body}</text:p></draw:text-box></draw:frame>`;
    const xml = [
      frame(2, '1cm', '1cm', '0.5cm', 'page 2'),
      frame(1, '5in', '1in', '0.2in', 'right'),
      frame(1, '1in', '0.5in', '0.3in', 'title &amp; &lt;x&gt; &#233;'),
      frame(1, '1in', '1in', '0.2in', 'a<text:s text:c="2"/>b'),
    ].join('');
    assert.strictEqual(fodtToText(xml), 'title & <x> é\na  b right\n\npage 2');
  });

  await test('input files are not locked after processing (can be deleted)', async () => {
    // Regression: the libvips cache used to hold on to a converted GIF
    await sharp({ create: { width: 50, height: 50, channels: 3, background: '#888' } }).gif().toFile(p('locked.gif'));
    await convert(p('locked.gif'), 'png');
    await runTool('image.rotate', [p('locked.gif')]);
    fs.rmSync(p('locked.gif'));
    fs.rmSync(p('photo.jpg')); // read by many tools above
  });

  // ---------- Error cases ----------
  await test('same format is skipped', () =>
    assert.rejects(convert(p('notes.txt'), 'txt'), (e) => e.skipped === true && e.message === 'Already a TXT file'));
  await test('jpeg counts as jpg (skipped)', async () => {
    fs.copyFileSync(p('picture.jpg'), p('copy.jpeg'));
    await assert.rejects(convert(p('copy.jpeg'), 'jpg'), (e) => e.skipped);
  });
  await test('image -> docx is rejected', () => rejects(convert(p('picture.png'), 'docx'), /Images can't be converted to DOCX/));
  await test('document -> jpg is rejected', () => rejects(convert(p('notes.txt'), 'jpg'), /Documents can't be converted to JPG/));
  await test('audio -> pdf is rejected', () => rejects(convert(p('tone.wav'), 'pdf'), /Audio files can't be converted to PDF/), noFf);
  await test('unsupported type', async () => {
    fs.writeFileSync(p('archive.zip'), 'x');
    await rejects(convert(p('archive.zip'), 'pdf'), /\.zip files aren't supported/);
  });
  await test('folders are rejected', () => rejects(convert(dir, 'pdf'), /Folders can't be processed/));
  await test('missing file', () => rejects(convert(p('missing.png'), 'jpg'), /File not found/));
  await test('unknown target format', () => rejects(convert(p('picture.png'), 'exe'), /Unknown target format/));
  await test('name clash -> (1), (2)', async () => {
    // picture.jpg already exists from the first test
    assert.strictEqual(await convert(p('picture.png'), 'jpg'), p('picture (1).jpg'));
    assert.strictEqual(await convert(p('picture.png'), 'jpg'), p('picture (2).jpg'));
    assert.ok(fs.existsSync(p('picture.jpg')), 'the existing file must not be overwritten');
  });

  fs.rmSync(dir, { recursive: true, force: true });
  console.log(`\n${passed} passed, ${failed} failed, ${skipped} skipped`);
  process.exit(failed ? 1 : 0);
})();
