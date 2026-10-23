// Tools satu-klik untuk roda Ctrl. Tidak bergantung pada Electron supaya bisa dites dengan node biasa.
const fs = require('fs');
const path = require('path');
const {
  sharp, pdfLib, AUDIO_ARGS, H264, extOf, baseOf, categoryOf, uniquePath, checkInput, withTmpDir, ffmpeg, ffmpegTo, probeDuration,
  sharpInput, imagesToPdf, viaLibreOffice, pdfToImages,
} = require('./converter');

// "foto.jpg" -> "foto (dikompres).jpg" di folder yang sama
const suffixed = (file, label, ext = extOf(file)) => uniquePath(path.dirname(file), `${baseOf(file)} (${label})`, ext);

function skip(message) {
  const err = new Error(message);
  err.skipped = true;
  return err;
}

// ---------- Gambar ----------

// Format yang bisa ditulis sharp; BMP/HEIC/SVG disimpan sebagai PNG
const SHARP_OUT = { jpg: 'jpeg', png: 'png', webp: 'webp', avif: 'avif', tiff: 'tiff', gif: 'gif' };

async function sharpTool(file, label, edit, encodeOpts = {}) {
  const ext = SHARP_OUT[extOf(file)] ? extOf(file) : 'png';
  const out = suffixed(file, label, ext);
  await withTmpDir(async (tmp) => {
    let img = sharp(await sharpInput(file, tmp), { animated: false }).autoOrient();
    img = await edit(img, await img.metadata());
    if (ext === 'jpg') img = img.flatten({ background: '#ffffff' });
    await img.toFormat(SHARP_OUT[ext], { quality: 90, ...encodeOpts[ext] }).toFile(out);
  });
  return out;
}

async function compressImage(file) {
  const out = await sharpTool(
    file,
    'dikompres',
    (img) => img.resize({ width: 2560, height: 2560, fit: 'inside', withoutEnlargement: true }),
    { jpg: { quality: 75, mozjpeg: true }, png: { palette: true, quality: 80, compressionLevel: 9 }, webp: { quality: 70 }, avif: { quality: 45 }, tiff: { compression: 'jpeg', quality: 75 } }
  );
  if (fs.statSync(out).size >= fs.statSync(file).size) {
    fs.rmSync(out);
    throw skip('Sudah optimal, hasil kompres tidak lebih kecil');
  }
  return out;
}

function squareCrop(img, meta) {
  const side = Math.min(meta.autoOrient.width, meta.autoOrient.height);
  return img.resize(side, side, { fit: 'cover', position: 'attention' }); // 'attention': pilih area paling menonjol
}

async function mergeImagesToPdf(files) {
  const out = uniquePath(path.dirname(files[0]), 'Gabungan', 'pdf');
  await imagesToPdf(files, out);
  return out;
}

// ---------- PDF ----------

async function loadPdf(file) {
  try {
    return await pdfLib().PDFDocument.load(fs.readFileSync(file), { updateMetadata: false });
  } catch {
    throw new Error('PDF tidak bisa dibaca (rusak atau terkunci password).');
  }
}

async function pdfTool(file, label, edit) {
  const doc = await loadPdf(file);
  await edit(doc);
  const out = suffixed(file, label);
  fs.writeFileSync(out, await doc.save());
  return out;
}

async function mergePdfs(files) {
  const merged = await pdfLib().PDFDocument.create();
  for (const file of files) {
    const doc = await loadPdf(file);
    for (const page of await merged.copyPages(doc, doc.getPageIndices())) merged.addPage(page);
  }
  const out = uniquePath(path.dirname(files[0]), 'Gabungan', 'pdf');
  fs.writeFileSync(out, await merged.save());
  return out;
}

async function splitPdf(file) {
  const doc = await loadPdf(file);
  const count = doc.getPageCount();
  if (count < 2) throw skip('PDF hanya 1 halaman');
  let first;
  for (let i = 0; i < count; i++) {
    const single = await pdfLib().PDFDocument.create();
    single.addPage((await single.copyPages(doc, [i]))[0]);
    const out = uniquePath(path.dirname(file), `${baseOf(file)} hal ${i + 1}`, 'pdf');
    fs.writeFileSync(out, await single.save());
    first = first || out;
  }
  return first;
}

function stripPdfMetadata(doc) {
  doc.setTitle('');
  doc.setAuthor('');
  doc.setSubject('');
  doc.setKeywords([]);
  doc.setCreator('');
  doc.setProducer('');
  doc.catalog.delete(pdfLib().PDFName.of('Metadata')); // XMP
}

// Nomor "1 / 5" kecil di tengah bawah tiap halaman.
// ponytail: halaman yang diputar (/Rotate) mendapat nomor di sisi "bawah" versi tak-terputar.
async function numberPages(doc) {
  const { StandardFonts, rgb } = pdfLib();
  const font = await doc.embedFont(StandardFonts.Helvetica);
  const pages = doc.getPages();
  pages.forEach((page, i) => {
    const text = `${i + 1} / ${pages.length}`;
    const size = 9;
    page.drawText(text, { x: (page.getWidth() - font.widthOfTextAtSize(text, size)) / 2, y: 18, size, font, color: rgb(0.4, 0.4, 0.4) });
  });
}

async function reversePdf(file) {
  const doc = await loadPdf(file);
  if (doc.getPageCount() < 2) throw skip('PDF hanya 1 halaman');
  const out = await pdfLib().PDFDocument.create();
  for (const page of await out.copyPages(doc, doc.getPageIndices().reverse())) out.addPage(page);
  const dest = suffixed(file, 'urutan terbalik');
  fs.writeFileSync(dest, await out.save());
  return dest;
}

// ---------- Dokumen, spreadsheet, presentasi (LibreOffice) ----------

async function mergeDocsToPdf(files) {
  const merged = await pdfLib().PDFDocument.create();
  await withTmpDir(async (tmp) => {
    for (const [i, file] of files.entries()) {
      const pdf = path.join(tmp, `${i}.pdf`);
      await viaLibreOffice(file, pdf, 'pdf');
      const doc = await loadPdf(pdf);
      for (const page of await merged.copyPages(doc, doc.getPageIndices())) merged.addPage(page);
    }
  });
  const out = uniquePath(path.dirname(files[0]), 'Gabungan', 'pdf');
  fs.writeFileSync(out, await merged.save());
  return out;
}

// Tiap halaman/slide jadi PNG: "laporan 1.png", "laporan 2.png" (satu halaman: "laporan (halaman).png")
function docPages(file) {
  return withTmpDir(async (tmp) => {
    const pdf = path.join(tmp, 'doc.pdf');
    await viaLibreOffice(file, pdf, 'pdf');
    return pdfToImages(pdf, suffixed(file, 'halaman', 'png'), 'png', file);
  });
}

const DOC_TOOLS = (pagesLabel, pagesCaption) => [
  { id: 'merge', icon: 'merge', label: 'Gabung PDF', caption: 'Gabung semua jadi satu PDF', multi: true, all: mergeDocsToPdf },
  { id: 'pages', icon: 'image', label: pagesLabel, caption: pagesCaption, each: docPages },
];

// ---------- Video & audio (ffmpeg) ----------

const H264_LIGHT = ['-c:v', 'libx264', '-preset', 'medium', '-crf', '28', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-b:a', '128k'];

function video(file, label, args, opts, ext = 'mp4') {
  const out = suffixed(file, label, ext);
  return ffmpegTo(file, args, out, opts.onProgress).then(() => out);
}

// Pakai codec format aslinya kalau ada, selain itu jadi MP3
function audio(file, label, args, opts, ext = AUDIO_ARGS[extOf(file)] ? extOf(file) : 'mp3') {
  const out = suffixed(file, label, ext);
  return ffmpegTo(file, ['-vn', ...args, ...AUDIO_ARGS[ext]], out, opts.onProgress).then(() => out);
}

// Progres ffmpeg diukur terhadap durasi input; kalau hasil dipercepat, skalakan supaya tetap sampai 100%
const faster = (opts, factor) => ({ onProgress: opts.onProgress && ((p) => opts.onProgress(Math.min(1, p * factor))) });

// Frame dari tengah video (-ss sebelum -i: lompat langsung, tidak mendekode separuh video)
async function videoThumb(file) {
  const out = suffixed(file, 'thumbnail', 'jpg');
  const duration = (await probeDuration(file)) || 0;
  await withTmpDir(async (tmp) => {
    const jpg = path.join(tmp, 'thumb.jpg');
    await ffmpeg(['-ss', (duration / 2).toFixed(2), '-i', file, '-frames:v', '1', '-q:v', '2', jpg]);
    fs.copyFileSync(jpg, out);
  });
  return out;
}

async function fadeAudio(file, opts) {
  const duration = (await probeDuration(file)) || 0;
  const d = Math.min(2, duration / 4); // klip pendek: fade lebih singkat
  const fade = `afade=t=in:d=${d.toFixed(2)},afade=t=out:st=${Math.max(0, duration - d).toFixed(2)}:d=${d.toFixed(2)}`;
  return audio(file, 'fade', ['-af', fade], opts);
}

// Buang hening di awal, balik, buang lagi (= hening di akhir), balik lagi
const TRIM_SILENCE = 'silenceremove=start_periods=1:start_threshold=-50dB,areverse,silenceremove=start_periods=1:start_threshold=-50dB,areverse';

// ---------- Daftar tools per kategori ----------
// each(file, opts): diproses per file. all(files): semua file jadi satu hasil (hanya tampil kalau file > 1).
// icon: nama ikon di overlay.html.

const TOOLS = {
  image: [
    { id: 'compress', icon: 'compress', label: 'Kompres', caption: 'Kompres gambar', each: compressImage },
    { id: 'half', icon: 'half', label: '50%', caption: 'Perkecil jadi 50%', each: (f) => sharpTool(f, '50%', (img, m) => img.resize(Math.max(1, Math.round(m.autoOrient.width / 2)))) },
    { id: 'rotate', icon: 'rotate', label: 'Putar', caption: 'Putar 90° searah jarum jam', each: (f) => sharpTool(f, 'diputar', (img) => img.rotate(90)) },
    { id: 'gray', icon: 'gray', label: 'Hitam putih', caption: 'Jadikan hitam putih', each: (f) => sharpTool(f, 'hitam putih', (img) => img.grayscale()) },
    { id: 'strip', icon: 'strip', label: 'Metadata', caption: 'Hapus metadata (EXIF, lokasi)', each: (f) => sharpTool(f, 'tanpa metadata', (img) => img) },
    { id: 'flip', icon: 'flip', label: 'Cermin', caption: 'Balik horizontal (cermin)', each: (f) => sharpTool(f, 'cermin', (img) => img.flop()) },
    { id: 'trim', icon: 'crop', label: 'Pangkas tepi', caption: 'Buang pinggiran polos di sekeliling', each: (f) => sharpTool(f, 'dipangkas', (img) => img.trim({ threshold: 12 })) },
    { id: 'square', icon: 'square', label: 'Persegi', caption: 'Potong jadi 1:1 di bagian paling menarik', each: (f) => sharpTool(f, 'persegi', squareCrop) },
    { id: 'merge', icon: 'merge', label: 'Gabung PDF', caption: 'Gabung jadi satu PDF', multi: true, all: mergeImagesToPdf },
  ],
  pdf: [
    { id: 'merge', icon: 'merge', label: 'Gabung', caption: 'Gabung jadi satu PDF', multi: true, all: mergePdfs },
    { id: 'split', icon: 'split', label: 'Pisah', caption: 'Pisah per halaman', each: splitPdf },
    { id: 'rotate', icon: 'rotate', label: 'Putar', caption: 'Putar semua halaman 90°', each: (f) => pdfTool(f, 'diputar', (doc) => doc.getPages().forEach((p) => p.setRotation(pdfLib().degrees((p.getRotation().angle + 90) % 360)))) },
    { id: 'strip', icon: 'strip', label: 'Metadata', caption: 'Hapus metadata PDF', each: (f) => pdfTool(f, 'tanpa metadata', stripPdfMetadata) },
    { id: 'number', icon: 'hash', label: 'Nomor hal.', caption: 'Tambah nomor halaman di bawah', each: (f) => pdfTool(f, 'bernomor', numberPages) },
    { id: 'reverse', icon: 'reverse', label: 'Balik urutan', caption: 'Balik urutan halaman (scan terbalik)', each: reversePdf },
  ],
  video: [
    { id: 'compress', icon: 'compress', label: 'Kompres', caption: 'Kompres video (maks 1080p)', each: (f, o) => video(f, 'dikompres', ['-vf', "scale=-2:'min(ih,1080)'", ...H264_LIGHT, '-movflags', '+faststart'], o) },
    { id: '720p', icon: 'screen', label: '720p', caption: 'Ubah ke 720p', each: (f, o) => video(f, '720p', ['-vf', 'scale=-2:720', ...H264, '-movflags', '+faststart'], o) },
    { id: 'mute', icon: 'mute', label: 'Bisukan', caption: 'Hapus suara', each: (f, o) => video(f, 'tanpa suara', ['-c:v', 'copy', '-an'], o, extOf(f)) },
    { id: 'rotate', icon: 'rotate', label: 'Putar', caption: 'Putar 90° searah jarum jam', each: (f, o) => video(f, 'diputar', ['-vf', 'transpose=1', ...H264], o) },
    { id: 'thumb', icon: 'image', label: 'Thumbnail', caption: 'Ambil gambar dari tengah video', each: videoThumb },
    { id: 'fast', icon: 'fast', label: '2× cepat', caption: 'Percepat 2 kali', each: (f, o) => video(f, '2x', ['-vf', 'setpts=0.5*PTS', '-af', 'atempo=2', ...H264], faster(o, 2)) },
  ],
  audio: [
    { id: 'compress', icon: 'compress', label: 'Kompres', caption: 'Kompres jadi MP3 128 kbps', each: (f, o) => { const out = suffixed(f, 'dikompres', 'mp3'); return ffmpegTo(f, ['-vn', '-c:a', 'libmp3lame', '-b:a', '128k'], out, o.onProgress).then(() => out); } },
    { id: 'normalize', icon: 'wave', label: 'Normalisasi', caption: 'Samakan volume (loudnorm)', each: (f, o) => audio(f, 'normal', ['-af', 'loudnorm=I=-16:TP=-1.5:LRA=11'], o) },
    { id: 'mono', icon: 'mono', label: 'Mono', caption: 'Jadikan mono', each: (f, o) => audio(f, 'mono', ['-ac', '1'], o) },
    { id: 'denoise', icon: 'denoise', label: 'Kurangi bising', caption: 'Kurangi desis dan bising latar', each: (f, o) => audio(f, 'bersih', ['-af', 'afftdn=nf=-25'], o) },
    { id: 'trim', icon: 'scissors', label: 'Potong hening', caption: 'Buang hening di awal dan akhir', each: (f, o) => audio(f, 'tanpa hening', ['-af', TRIM_SILENCE], o) },
    { id: 'fast', icon: 'fast', label: '1,5× cepat', caption: 'Percepat 1,5 kali, nada tetap', each: (f, o) => audio(f, '1,5x', ['-af', 'atempo=1.5'], faster(o, 1.5)) },
    { id: 'fade', icon: 'fade', label: 'Fade', caption: 'Fade-in dan fade-out di ujung', each: fadeAudio },
  ],
  document: DOC_TOOLS('Per halaman', 'Simpan tiap halaman jadi PNG'),
  sheet: DOC_TOOLS('Per halaman', 'Simpan tiap halaman jadi PNG'),
  slides: DOC_TOOLS('Per slide', 'Simpan tiap slide jadi PNG'),
};

/**
 * Ubah satu pilihan tool ("image.compress") + daftar file jadi daftar tugas antrean.
 * Tiap tugas: { name, label, chip, run: (opts) => Promise<path hasil> }.
 */
function toolTasks(toolId, files) {
  const [cat, id] = String(toolId).split('.');
  const tool = (TOOLS[cat] || []).find((t) => t.id === id);
  if (!tool) throw new Error(`Tool tidak dikenal: ${toolId}`);

  const ofCat = (f) => {
    checkInput(f);
    if (categoryOf(f) !== cat) throw new Error(`Tool "${tool.caption}" tidak berlaku untuk file .${extOf(f)}`);
  };
  const info = { label: tool.caption, chip: tool.label };

  if (tool.all) {
    return [{
      ...info,
      name: `${files.length} file`,
      run: async () => {
        files.forEach(ofCat);
        if (files.length < 2) throw new Error('Pilih minimal 2 file untuk digabung.');
        return tool.all(files);
      },
    }];
  }
  return files.map((f) => ({ ...info, name: path.basename(String(f || '')), run: async (opts = {}) => { ofCat(f); return tool.each(f, opts); } }));
}

module.exports = { TOOLS, toolTasks };
