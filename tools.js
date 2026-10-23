// Tools satu-klik untuk roda Ctrl. Tidak bergantung pada Electron supaya bisa dites dengan node biasa.
const fs = require('fs');
const path = require('path');
const sharp = require('sharp');
const { PDFDocument, PDFName, degrees } = require('pdf-lib');
const {
  AUDIO_ARGS, H264, extOf, baseOf, categoryOf, uniquePath, checkInput, withTmpDir, ffmpegTo, sharpInput, imagesToPdf,
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

async function mergeImagesToPdf(files) {
  const out = uniquePath(path.dirname(files[0]), 'Gabungan', 'pdf');
  await imagesToPdf(files, out);
  return out;
}

// ---------- PDF ----------

async function loadPdf(file) {
  try {
    return await PDFDocument.load(fs.readFileSync(file), { updateMetadata: false });
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
  const merged = await PDFDocument.create();
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
    const single = await PDFDocument.create();
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
  doc.catalog.delete(PDFName.of('Metadata')); // XMP
}

// ---------- Video & audio (ffmpeg) ----------

const H264_LIGHT = ['-c:v', 'libx264', '-preset', 'medium', '-crf', '28', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-b:a', '128k'];

function video(file, label, args, ext = 'mp4') {
  const out = suffixed(file, label, ext);
  return ffmpegTo(file, args, out).then(() => out);
}

// Pakai codec format aslinya kalau ada, selain itu jadi MP3
function audio(file, label, args) {
  const ext = AUDIO_ARGS[extOf(file)] ? extOf(file) : 'mp3';
  const out = suffixed(file, label, ext);
  return ffmpegTo(file, ['-vn', ...args, ...AUDIO_ARGS[ext]], out).then(() => out);
}

// ---------- Daftar tools per kategori ----------
// each: diproses per file. all: semua file jadi satu hasil (hanya tampil kalau file > 1).

const TOOLS = {
  image: [
    { id: 'compress', label: 'Kompres', caption: 'Kompres gambar', each: compressImage },
    { id: 'half', label: '50%', caption: 'Perkecil jadi 50%', each: (f) => sharpTool(f, '50%', (img, m) => img.resize(Math.max(1, Math.round(m.autoOrient.width / 2)))) },
    { id: 'rotate', label: 'Putar', caption: 'Putar 90° searah jarum jam', each: (f) => sharpTool(f, 'diputar', (img) => img.rotate(90)) },
    { id: 'gray', label: 'Hitam Putih', caption: 'Jadikan hitam putih', each: (f) => sharpTool(f, 'hitam putih', (img) => img.grayscale()) },
    { id: 'strip', label: 'Metadata', caption: 'Hapus metadata (EXIF, lokasi)', each: (f) => sharpTool(f, 'tanpa metadata', (img) => img) },
    { id: 'merge', label: 'Gabung PDF', caption: 'Gabung jadi satu PDF', multi: true, all: mergeImagesToPdf },
  ],
  pdf: [
    { id: 'merge', label: 'Gabung', caption: 'Gabung jadi satu PDF', multi: true, all: mergePdfs },
    { id: 'split', label: 'Pisah', caption: 'Pisah per halaman', each: splitPdf },
    { id: 'rotate', label: 'Putar', caption: 'Putar semua halaman 90°', each: (f) => pdfTool(f, 'diputar', (doc) => doc.getPages().forEach((p) => p.setRotation(degrees((p.getRotation().angle + 90) % 360)))) },
    { id: 'strip', label: 'Metadata', caption: 'Hapus metadata PDF', each: (f) => pdfTool(f, 'tanpa metadata', stripPdfMetadata) },
  ],
  video: [
    { id: 'compress', label: 'Kompres', caption: 'Kompres video (maks 1080p)', each: (f) => video(f, 'dikompres', ['-vf', "scale=-2:'min(ih,1080)'", ...H264_LIGHT, '-movflags', '+faststart']) },
    { id: '720p', label: '720p', caption: 'Ubah ke 720p', each: (f) => video(f, '720p', ['-vf', 'scale=-2:720', ...H264, '-movflags', '+faststart']) },
    { id: 'mute', label: 'Bisukan', caption: 'Hapus suara', each: (f) => video(f, 'tanpa suara', ['-c:v', 'copy', '-an'], extOf(f)) },
    { id: 'rotate', label: 'Putar', caption: 'Putar 90° searah jarum jam', each: (f) => video(f, 'diputar', ['-vf', 'transpose=1', ...H264]) },
  ],
  audio: [
    { id: 'compress', label: 'Kompres', caption: 'Kompres jadi MP3 128 kbps', each: (f) => { const out = suffixed(f, 'dikompres', 'mp3'); return ffmpegTo(f, ['-vn', '-c:a', 'libmp3lame', '-b:a', '128k'], out).then(() => out); } },
    { id: 'normalize', label: 'Normalisasi', caption: 'Samakan volume (loudnorm)', each: (f) => audio(f, 'normal', ['-af', 'loudnorm=I=-16:TP=-1.5:LRA=11']) },
    { id: 'mono', label: 'Mono', caption: 'Jadikan mono', each: (f) => audio(f, 'mono', ['-ac', '1']) },
  ],
};

/**
 * Ubah satu pilihan tool ("image.compress") + daftar file jadi daftar tugas antrean.
 * Tiap tugas: { name, run: () => Promise<path hasil> }.
 */
function toolTasks(toolId, files) {
  const [cat, id] = String(toolId).split('.');
  const tool = (TOOLS[cat] || []).find((t) => t.id === id);
  if (!tool) throw new Error(`Tool tidak dikenal: ${toolId}`);

  const ofCat = (f) => {
    checkInput(f);
    if (categoryOf(f) !== cat) throw new Error(`Tool "${tool.caption}" tidak berlaku untuk file .${extOf(f)}`);
  };

  if (tool.all) {
    return [{
      name: tool.caption,
      run: async () => {
        files.forEach(ofCat);
        if (files.length < 2) throw new Error('Pilih minimal 2 file untuk digabung.');
        return tool.all(files);
      },
    }];
  }
  return files.map((f) => ({ name: path.basename(String(f || '')), run: async () => { ofCat(f); return tool.each(f); } }));
}

module.exports = { TOOLS, toolTasks };
