// Konversi file. Tidak bergantung pada Electron supaya bisa dites dengan node biasa.
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFile } = require('child_process');
const { pathToFileURL } = require('url');
const sharp = require('sharp');
const { PDFDocument } = require('pdf-lib');

const FORMATS = ['pdf', 'docx', 'txt', 'jpg'];
const DOC_EXT = ['doc', 'docx', 'odt', 'rtf', 'txt', 'html'];
const IMG_EXT = ['png', 'jpg', 'webp', 'gif', 'tiff', 'avif'];
const ALIAS = { jpeg: 'jpg', tif: 'tiff', htm: 'html' };

// Filter ekspor LibreOffice per format tujuan
const LO_TARGET = {
  pdf: 'pdf',
  docx: 'docx:MS Word 2007 XML',
  txt: 'txt:Text (encoded):UTF8',
};

function extOf(file) {
  const e = path.extname(file).slice(1).toLowerCase();
  return ALIAS[e] || e;
}

// Cari nama yang belum dipakai: nama.ext, nama (1).ext, nama (2).ext, ...
function uniquePath(dir, base, ext) {
  let candidate = path.join(dir, `${base}.${ext}`);
  for (let i = 1; fs.existsSync(candidate); i++) {
    candidate = path.join(dir, `${base} (${i}).${ext}`);
  }
  return candidate;
}

function run(cmd, args, timeout = 120000) {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, { timeout, windowsHide: true }, (err, stdout, stderr) => {
      if (err) {
        err.stderr = stderr;
        return reject(err);
      }
      resolve(stdout);
    });
  });
}

function findSoffice() {
  const roots = [
    process.env.ProgramFiles,
    process.env['ProgramFiles(x86)'],
    process.env.LOCALAPPDATA && path.join(process.env.LOCALAPPDATA, 'Programs'),
    process.env.LOCALAPPDATA,
  ].filter(Boolean);
  for (const root of roots) {
    const exe = path.join(root, 'LibreOffice', 'program', 'soffice.exe');
    if (fs.existsSync(exe)) return exe;
  }
  return 'soffice'; // fallback ke PATH
}

async function withTmpDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'shiftwheel-'));
  try {
    return await fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

const UNIT_PT = { in: 72, cm: 72 / 2.54, mm: 72 / 25.4, pt: 1 };
const ENTITY = { lt: '<', gt: '>', quot: '"', apos: "'", amp: '&' };

const attr = (tag, name) => (new RegExp(`${name}="([^"]*)"`).exec(tag) || [])[1];
function toPt(value) {
  const m = /^(-?[\d.]+)(in|cm|mm|pt)$/.exec(value || '');
  return m ? m[1] * UNIT_PT[m[2]] : 0;
}

// Isi teks dari potongan XML ODF
function xmlText(xml) {
  return xml
    .replace(/<text:s text:c="(\d+)"\/>/g, (_, n) => ' '.repeat(n))
    .replace(/<text:s\/>/g, ' ')
    .replace(/<text:tab\/>/g, '\t')
    .replace(/<text:line-break\/>/g, '\n')
    .replace(/<\/text:[ph]>/g, '\n')
    .replace(/<[^>]+>/g, '')
    .replace(/&(#x[0-9a-f]+|#\d+|\w+);/gi, (m, e) => {
      if (e[0] !== '#') return ENTITY[e] || m;
      return String.fromCodePoint(e[1].toLowerCase() === 'x' ? parseInt(e.slice(2), 16) : Number(e.slice(1)));
    })
    .trim();
}

// Import PDF di Writer menaruh tiap potongan teks di frame terpisah dengan urutan acak,
// dan ekspor TXT mengabaikan frame. Jadi teks diambil dari FODT lalu diurutkan sesuai posisi.
// ponytail: tata letak kolom/tabel jadi baris berurutan kiri->kanan, cukup untuk teks biasa.
function fodtToText(xml) {
  const frames = [...xml.matchAll(/<draw:frame\b([^>]*)>([\s\S]*?)<\/draw:frame>/g)]
    .map(([, a, body]) => ({
      page: Number(attr(a, 'text:anchor-page-number')) || 0,
      x: toPt(attr(a, 'svg:x')),
      bottom: toPt(attr(a, 'svg:y')) + toPt(attr(a, 'svg:height')), // dekat baseline, stabil antar ukuran font
      // hanya isi text-box; frame gambar menyimpan data base64 yang bukan teks
      text: xmlText((/<draw:text-box\b[^>]*>([\s\S]*?)<\/draw:text-box>/.exec(body) || [])[1] || ''),
    }))
    .filter((f) => f.text)
    .sort((a, b) => a.page - b.page || a.bottom - b.bottom || a.x - b.x);

  const out = [];
  let line = [];
  const flush = () => {
    if (line.length) out.push(line.sort((a, b) => a.x - b.x).map((f) => f.text).join(' '));
    line = [];
  };
  for (const f of frames) {
    const prev = line[line.length - 1];
    if (prev && (f.page !== prev.page || Math.abs(f.bottom - prev.bottom) > 3)) {
      const newPage = f.page !== prev.page;
      flush();
      if (newPage) out.push('');
    }
    line.push(f);
  }
  flush();
  return out.join('\n');
}

async function viaLibreOffice(input, fmt, out, infilter) {
  // Profil terpisah supaya tidak bentrok dengan LibreOffice yang sedang dibuka user
  const profile = pathToFileURL(path.join(os.tmpdir(), 'shiftwheel-lo-profile')).href;
  const pdfText = infilter === 'writer_pdf_import' && fmt === 'txt';
  const ext = pdfText ? 'fodt' : fmt;
  await withTmpDir(async (tmp) => {
    const args = [
      `-env:UserInstallation=${profile}`,
      '--headless', '--norestore', '--nolockcheck',
      ...(infilter ? [`--infilter=${infilter}`] : []),
      '--convert-to', pdfText ? 'fodt' : LO_TARGET[fmt],
      '--outdir', tmp,
      input,
    ];
    try {
      await run(findSoffice(), args);
    } catch (err) {
      if (err.code === 'ENOENT') throw new Error('LibreOffice tidak ditemukan. Install LibreOffice dulu.');
      if (err.killed) throw new Error('LibreOffice terlalu lama merespons (lebih dari 2 menit).');
      throw new Error('LibreOffice gagal mengonversi file.');
    }
    const produced = fs.readdirSync(tmp).find((f) => extOf(f) === ext);
    if (!produced) throw new Error('LibreOffice tidak menghasilkan file. Pastikan file tidak rusak atau terkunci.');
    if (!pdfText) return fs.copyFileSync(path.join(tmp, produced), out);
    const text = fodtToText(fs.readFileSync(path.join(tmp, produced), 'utf8'));
    if (!text) throw new Error('PDF tidak berisi teks yang bisa dibaca (mungkin hasil scan).');
    fs.writeFileSync(out, text + '\n');
  });
}

async function imageToJpg(input, out) {
  await sharp(input, { animated: false })
    .rotate() // ikuti orientasi EXIF
    .flatten({ background: '#ffffff' })
    .jpeg({ quality: 92 })
    .toFile(out);
}

async function imageToPdf(input, out) {
  const isJpg = extOf(input) === 'jpg';
  const img = sharp(input, { animated: false }).rotate();
  // pdf-lib hanya bisa embed JPG/PNG: JPG tetap JPG, sisanya jadi PNG (lossless)
  const { data, info } = await (isJpg ? img.jpeg({ quality: 92 }) : img.png()).toBuffer({ resolveWithObject: true });
  const doc = await PDFDocument.create();
  const embedded = isJpg ? await doc.embedJpg(data) : await doc.embedPng(data);
  const width = 595;
  const height = (width * info.height) / info.width;
  doc.addPage([width, height]).drawImage(embedded, { x: 0, y: 0, width, height });
  fs.writeFileSync(out, await doc.save());
}

async function pdfToJpg(input, out) {
  await withTmpDir(async (tmp) => {
    const prefix = path.join(tmp, 'page');
    try {
      await run('pdftoppm', ['-jpeg', '-r', '150', '-singlefile', '-f', '1', '-l', '1', input, prefix]);
    } catch (err) {
      if (err.code === 'ENOENT') throw new Error('pdftoppm tidak ditemukan. Install Poppler untuk konversi PDF ke JPG.');
      throw new Error('pdftoppm gagal mengonversi PDF.');
    }
    fs.copyFileSync(`${prefix}.jpg`, out);
  });
}

// Pilih metode konversi, atau lempar error ramah kalau kombinasinya tidak didukung
function pickConverter(src, fmt) {
  if (DOC_EXT.includes(src) && fmt !== 'jpg') return (i, o) => viaLibreOffice(i, fmt, o);
  if (src === 'pdf' && (fmt === 'docx' || fmt === 'txt')) return (i, o) => viaLibreOffice(i, fmt, o, 'writer_pdf_import');
  if (src === 'pdf' && fmt === 'jpg') return pdfToJpg;
  if (IMG_EXT.includes(src) && fmt === 'jpg') return imageToJpg;
  if (IMG_EXT.includes(src) && fmt === 'pdf') return imageToPdf;

  if (IMG_EXT.includes(src)) throw new Error(`Gambar tidak bisa dikonversi ke ${fmt.toUpperCase()}. Pilih JPG atau PDF.`);
  if (DOC_EXT.includes(src)) throw new Error('Dokumen tidak bisa dikonversi langsung ke JPG. Konversi ke PDF dulu.');
  throw new Error(src ? `Tipe file .${src} tidak didukung.` : 'File tanpa ekstensi tidak didukung.');
}

/**
 * Konversi satu file ke format tujuan. Hasil disimpan di folder yang sama.
 * Mengembalikan path hasil. Error dengan `skipped = true` berarti file dilewati.
 */
async function convert(input, fmt) {
  fmt = String(fmt || '').toLowerCase();
  if (!FORMATS.includes(fmt)) throw new Error(`Format tujuan tidak dikenal: ${fmt}`);
  if (typeof input !== 'string' || !fs.existsSync(input)) {
    throw new Error(`File tidak ditemukan: ${path.basename(String(input || ''))}`);
  }
  if (!fs.statSync(input).isFile()) throw new Error('Folder tidak bisa dikonversi. Pilih file.');

  const src = extOf(input);
  if (src === fmt) {
    const err = new Error(`Sudah berformat ${fmt.toUpperCase()}`);
    err.skipped = true;
    throw err;
  }

  const converter = pickConverter(src, fmt);
  const out = uniquePath(path.dirname(input), path.basename(input, path.extname(input)), fmt);
  await converter(input, out);
  return out;
}

module.exports = { convert, findSoffice, uniquePath, fodtToText, FORMATS };
