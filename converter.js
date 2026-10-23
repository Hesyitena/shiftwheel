// Konversi file. Tidak bergantung pada Electron supaya bisa dites dengan node biasa.
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFile, spawn } = require('child_process');
const { pathToFileURL } = require('url');
const sharp = require('sharp');
const { PDFDocument } = require('pdf-lib');

// Cache libvips membiarkan file input tetap terbuka; di Windows file itu jadi tidak bisa
// dihapus/dipindah selama Shiftwheel jalan. Tiap file cuma diproses sekali, jadi cache tidak perlu.
sharp.cache(false);

// Kategori file: ekstensi sumber dan format tujuan yang tampil di roda konversi
const CATEGORIES = {
  image: { name: 'Gambar', exts: ['png', 'jpg', 'webp', 'gif', 'tiff', 'avif', 'bmp', 'heic', 'svg'], targets: ['jpg', 'png', 'webp', 'avif', 'tiff', 'gif', 'bmp', 'pdf'] },
  pdf: { name: 'PDF', exts: ['pdf'], targets: ['docx', 'txt', 'jpg', 'png'] },
  document: { name: 'Dokumen', exts: ['doc', 'docx', 'odt', 'rtf', 'txt', 'html'], targets: ['pdf', 'docx', 'odt', 'rtf', 'txt', 'html'] },
  sheet: { name: 'Spreadsheet', exts: ['xls', 'xlsx', 'ods', 'csv'], targets: ['pdf', 'xlsx', 'ods', 'csv'] },
  slides: { name: 'Presentasi', exts: ['ppt', 'pptx', 'odp'], targets: ['pdf', 'pptx', 'odp'] },
  audio: { name: 'Audio', exts: ['mp3', 'm4a', 'wav', 'flac', 'ogg', 'opus', 'aiff', 'wma', 'aac'], targets: ['mp3', 'm4a', 'wav', 'flac', 'ogg', 'opus', 'aiff'] },
  video: { name: 'Video', exts: ['mp4', 'mov', 'mkv', 'webm', 'avi', 'wmv', 'm4v'], targets: ['mp4', 'mov', 'mkv', 'webm', 'gif', 'mp3'] },
};
// Roda saat tipe file tidak diketahui (Windows tidak selalu memberi MIME): format paling umum
const FALLBACK_TARGETS = ['pdf', 'docx', 'txt', 'jpg', 'png', 'mp3', 'mp4'];
const ALIAS = { jpeg: 'jpg', tif: 'tiff', htm: 'html', aif: 'aiff', heif: 'heic' };

// Filter ekspor LibreOffice per format tujuan
const LO_FILTER = {
  pdf: 'pdf',
  docx: 'docx:MS Word 2007 XML',
  odt: 'odt',
  rtf: 'rtf:Rich Text Format',
  txt: 'txt:Text (encoded):UTF8',
  html: 'html:XHTML Writer File:UTF8',
  xlsx: 'xlsx:Calc MS Excel 2007 XML',
  ods: 'ods',
  csv: 'csv:Text - txt - csv (StarCalc):44,34,76',
  pptx: 'pptx:Impress MS PowerPoint 2007 XML',
  odp: 'odp',
};

// Argumen ffmpeg per format tujuan
const AUDIO_ARGS = {
  mp3: ['-c:a', 'libmp3lame', '-q:a', '2'],
  m4a: ['-c:a', 'aac', '-b:a', '192k'],
  wav: ['-c:a', 'pcm_s16le'],
  flac: ['-c:a', 'flac'],
  ogg: ['-c:a', 'libvorbis', '-q:a', '5'],
  opus: ['-c:a', 'libopus', '-b:a', '128k'],
  aiff: ['-c:a', 'pcm_s16be'],
};
const H264 = ['-c:v', 'libx264', '-preset', 'medium', '-crf', '20', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-b:a', '160k'];
const VIDEO_ARGS = {
  mp4: [...H264, '-movflags', '+faststart'],
  mov: H264,
  mkv: H264,
  webm: ['-c:v', 'libvpx-vp9', '-crf', '32', '-b:v', '0', '-row-mt', '1', '-c:a', 'libopus', '-b:a', '128k'],
  gif: ['-an', '-vf', "fps=12,scale='min(480,iw)':-2:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse", '-loop', '0'],
  mp3: ['-vn', ...AUDIO_ARGS.mp3],
};

// ---------- Utilitas ----------

function extOf(file) {
  const e = path.extname(file).slice(1).toLowerCase();
  return ALIAS[e] || e;
}

const baseOf = (file) => path.basename(file, path.extname(file));

function categoryOf(file) {
  const ext = extOf(file);
  return Object.keys(CATEGORIES).find((k) => CATEGORIES[k].exts.includes(ext)) || null;
}

// Cari nama yang belum dipakai: nama.ext, nama (1).ext, nama (2).ext, ... (ext kosong untuk folder)
function uniquePath(dir, base, ext) {
  const name = (b) => (ext ? `${b}.${ext}` : b);
  let candidate = path.join(dir, name(base));
  for (let i = 1; fs.existsSync(candidate); i++) candidate = path.join(dir, name(`${base} (${i})`));
  return candidate;
}

function run(cmd, args, timeout = 120000) {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, { timeout, windowsHide: true, maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) {
        err.stderr = String(stderr || '');
        return reject(err);
      }
      resolve(stdout);
    });
  });
}

const readdir = (dir) => { try { return fs.readdirSync(dir); } catch { return []; } };

// Cari program bantu: folder install umum, paket winget (belum tentu masuk PATH proses ini), lalu PATH
function findTool(name, roots = []) {
  const winget = process.env.LOCALAPPDATA && path.join(process.env.LOCALAPPDATA, 'Microsoft', 'WinGet');
  const candidates = [...roots];
  if (winget) {
    candidates.push(path.join(winget, 'Links', `${name}.exe`));
    const pkgs = path.join(winget, 'Packages');
    for (const pkg of readdir(pkgs)) {
      for (const sub of readdir(path.join(pkgs, pkg))) {
        candidates.push(path.join(pkgs, pkg, sub, 'bin', `${name}.exe`), path.join(pkgs, pkg, sub, 'Library', 'bin', `${name}.exe`));
      }
    }
  }
  return candidates.find((c) => fs.existsSync(c)) || name;
}

function findSoffice() {
  const roots = [
    process.env.ProgramFiles,
    process.env['ProgramFiles(x86)'],
    process.env.LOCALAPPDATA && path.join(process.env.LOCALAPPDATA, 'Programs'),
    process.env.LOCALAPPDATA,
  ].filter(Boolean);
  return findTool('soffice', roots.map((r) => path.join(r, 'LibreOffice', 'program', 'soffice.exe')));
}

async function withTmpDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'shiftwheel-'));
  try {
    return await fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

const lastLine = (s) => String(s || '').trim().split(/\r?\n/).pop().slice(0, 160);

// ---------- ffmpeg ----------

async function probeDuration(input) {
  try {
    const out = await run(findTool('ffprobe'), ['-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', input]);
    return Number(String(out).trim()) || null;
  } catch {
    return null; // tanpa durasi, progres tampil tak tentu
  }
}

// onProgress(0..1) dipanggil dari laporan -progress ffmpeg kalau durasi input diketahui
async function ffmpeg(args, onProgress) {
  const duration = onProgress ? await probeDuration(args[args.indexOf('-i') + 1]) : null;
  await new Promise((resolve, reject) => {
    // Tanpa timeout: video panjang bisa makan waktu lama
    const child = spawn(findTool('ffmpeg'), ['-hide_banner', '-loglevel', 'error', '-nostdin', '-y', '-progress', 'pipe:1', '-nostats', ...args], { windowsHide: true });
    let stderr = '';
    let pending = '';
    child.stderr.on('data', (d) => { stderr = (stderr + d).slice(-4000); });
    child.stdout.on('data', (d) => {
      const lines = (pending + d).split('\n');
      pending = lines.pop();
      for (const line of lines) {
        const m = /^out_time_us=(\d+)/.exec(line);
        if (m && duration) onProgress(Math.min(1, Number(m[1]) / 1e6 / duration));
      }
    });
    child.on('error', (err) => {
      reject(err.code === 'ENOENT' ? new Error('ffmpeg tidak ditemukan. Install dulu: winget install Gyan.FFmpeg') : err);
    });
    child.on('close', (code) => {
      if (code === 0) resolve();
      else reject(new Error(`ffmpeg gagal memproses file. ${lastLine(stderr)}`.trim()));
    });
  });
}

// Tulis ke folder tmp dulu supaya file setengah jadi tidak tertinggal kalau gagal
async function ffmpegTo(input, args, out, onProgress) {
  await withTmpDir(async (tmp) => {
    const tmpOut = path.join(tmp, `out${path.extname(out)}`);
    await ffmpeg(['-i', input, ...args, tmpOut], onProgress);
    fs.copyFileSync(tmpOut, out);
  });
}

// ---------- LibreOffice ----------

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

async function viaLibreOffice(input, out, fmt) {
  // Profil terpisah supaya tidak bentrok dengan LibreOffice yang sedang dibuka user
  const profile = pathToFileURL(path.join(os.tmpdir(), 'shiftwheel-lo-profile')).href;
  const src = extOf(input);
  const infilter = src === 'pdf' ? 'writer_pdf_import' : src === 'csv' ? 'CSV:44,34,76' : null;
  const pdfText = src === 'pdf' && fmt === 'txt';
  const ext = pdfText ? 'fodt' : fmt;
  await withTmpDir(async (tmp) => {
    const args = [
      `-env:UserInstallation=${profile}`,
      '--headless', '--norestore', '--nolockcheck',
      ...(infilter ? [`--infilter=${infilter}`] : []),
      '--convert-to', pdfText ? 'fodt' : LO_FILTER[fmt],
      '--outdir', tmp,
      input,
    ];
    // Coba ulang sekali: start pertama LibreOffice (membuat profil) atau profil yang sedang
    // dipakai proses lain kadang gagal tanpa alasan jelas, percobaan kedua biasanya berhasil.
    for (let attempt = 1; ; attempt++) {
      try {
        await run(findSoffice(), args);
      } catch (err) {
        if (err.code === 'ENOENT') throw new Error('LibreOffice tidak ditemukan. Install LibreOffice dulu.');
        if (err.killed) throw new Error('LibreOffice terlalu lama merespons (lebih dari 2 menit).');
        if (attempt < 2) continue;
        throw new Error(`LibreOffice gagal mengonversi file. ${lastLine(err.stderr)}`.trim());
      }
      if (attempt >= 2 || readdir(tmp).some((f) => extOf(f) === ext)) break;
    }
    const produced = readdir(tmp).find((f) => extOf(f) === ext);
    if (!produced) throw new Error('LibreOffice tidak menghasilkan file. Pastikan file tidak rusak atau terkunci.');
    if (!pdfText) return fs.copyFileSync(path.join(tmp, produced), out);
    const text = fodtToText(fs.readFileSync(path.join(tmp, produced), 'utf8'));
    if (!text) throw new Error('PDF tidak berisi teks yang bisa dibaca (mungkin hasil scan).');
    fs.writeFileSync(out, text + '\n');
  });
}

// ---------- Gambar ----------

// sharp tidak bisa membaca BMP/HEIC: ubah dulu ke PNG lewat ffmpeg
async function sharpInput(input, tmp) {
  if (!['bmp', 'heic'].includes(extOf(input))) return input;
  const png = path.join(tmp, `${path.basename(input)}.png`);
  await ffmpeg(['-i', input, '-frames:v', '1', png]);
  return png;
}

const ENCODE = {
  jpg: (img) => img.flatten({ background: '#ffffff' }).jpeg({ quality: 92, mozjpeg: true }),
  png: (img) => img.png(),
  webp: (img) => img.webp({ quality: 90 }),
  avif: (img) => img.avif({ quality: 60 }),
  tiff: (img) => img.tiff(),
  gif: (img) => img.gif(),
};

// Satu atau beberapa gambar jadi satu PDF, satu halaman per gambar (lebar 595 pt)
async function imagesToPdf(files, out) {
  const doc = await PDFDocument.create();
  await withTmpDir(async (tmp) => {
    for (const file of files) {
      const isJpg = extOf(file) === 'jpg';
      const img = sharp(await sharpInput(file, tmp), { animated: false }).autoOrient();
      // pdf-lib hanya bisa embed JPG/PNG: JPG tetap JPG, sisanya jadi PNG (lossless)
      const { data, info } = await (isJpg ? img.jpeg({ quality: 92 }) : img.png()).toBuffer({ resolveWithObject: true });
      const embedded = isJpg ? await doc.embedJpg(data) : await doc.embedPng(data);
      const width = 595;
      const height = (width * info.height) / info.width;
      doc.addPage([width, height]).drawImage(embedded, { x: 0, y: 0, width, height });
    }
  });
  fs.writeFileSync(out, await doc.save());
}

async function convertImage(input, out, fmt) {
  if (fmt === 'pdf') return imagesToPdf([input], out);
  await withTmpDir(async (tmp) => {
    const src = await sharpInput(input, tmp);
    // Animasi GIF/WebP dipertahankan kalau tujuannya juga mendukung animasi
    const animated = ['gif', 'webp'].includes(fmt) && ['gif', 'webp'].includes(extOf(input));
    let img = sharp(src, { animated });
    if (!animated) img = img.autoOrient(); // ikuti orientasi EXIF
    if (fmt === 'bmp') {
      const png = path.join(tmp, 'flat.png');
      await img.flatten({ background: '#ffffff' }).png().toFile(png);
      return ffmpegTo(png, [], out);
    }
    await ENCODE[fmt](img).toFile(out);
  });
}

// ---------- PDF ke gambar ----------

async function pdfToImages(input, out, fmt) {
  return withTmpDir(async (tmp) => {
    try {
      await run(findTool('pdftoppm'), [fmt === 'jpg' ? '-jpeg' : '-png', '-r', '150', input, path.join(tmp, 'p')]);
    } catch (err) {
      if (err.code === 'ENOENT') throw new Error('pdftoppm tidak ditemukan. Install Poppler dulu: winget install oschwartz10612.Poppler');
      throw new Error('pdftoppm gagal membaca PDF (rusak atau terkunci password).');
    }
    const pages = readdir(tmp).sort(); // p-01.jpg, p-02.jpg, ... (nomor sudah di-pad)
    if (pages.length === 1) {
      fs.copyFileSync(path.join(tmp, pages[0]), out);
      return out;
    }
    // Banyak halaman: "nama 1.jpg", "nama 2.jpg", ...
    const outs = pages.map((p, i) => {
      const dest = uniquePath(path.dirname(input), `${baseOf(input)} ${i + 1}`, fmt);
      fs.copyFileSync(path.join(tmp, p), dest);
      return dest;
    });
    return outs[0];
  });
}

// ---------- Konversi utama ----------

const CONVERTERS = {
  image: convertImage,
  pdf: (i, o, fmt) => (fmt === 'jpg' || fmt === 'png' ? pdfToImages(i, o, fmt) : viaLibreOffice(i, o, fmt)),
  document: viaLibreOffice,
  sheet: viaLibreOffice,
  slides: viaLibreOffice,
  audio: (i, o, fmt, opts) => ffmpegTo(i, ['-vn', ...AUDIO_ARGS[fmt]], o, opts.onProgress),
  video: (i, o, fmt, opts) => ffmpegTo(i, VIDEO_ARGS[fmt], o, opts.onProgress),
};

function checkInput(input) {
  if (typeof input !== 'string' || !fs.existsSync(input)) {
    throw new Error(`File tidak ditemukan: ${path.basename(String(input || ''))}`);
  }
  if (!fs.statSync(input).isFile()) throw new Error('Folder tidak bisa diproses. Pilih file.');
}

/**
 * Konversi satu file ke format tujuan. Hasil disimpan di folder yang sama.
 * Mengembalikan path hasil (halaman pertama untuk PDF multi-halaman).
 * Error dengan `skipped = true` berarti file dilewati.
 * opts.onProgress(0..1) dipanggil selama konversi audio/video.
 */
async function convert(input, fmt, opts = {}) {
  fmt = ALIAS[String(fmt || '').toLowerCase()] || String(fmt || '').toLowerCase();
  const allTargets = new Set(Object.values(CATEGORIES).flatMap((c) => c.targets));
  if (!allTargets.has(fmt)) throw new Error(`Format tujuan tidak dikenal: ${fmt}`);
  checkInput(input);

  const src = extOf(input);
  if (src === fmt) {
    const err = new Error(`Sudah berformat ${fmt.toUpperCase()}`);
    err.skipped = true;
    throw err;
  }

  const cat = categoryOf(input);
  if (!cat) throw new Error(src ? `Tipe file .${src} tidak didukung.` : 'File tanpa ekstensi tidak didukung.');
  if (!CATEGORIES[cat].targets.includes(fmt)) {
    throw new Error(`${CATEGORIES[cat].name} tidak bisa dikonversi ke ${fmt.toUpperCase()}.`);
  }

  const out = uniquePath(path.dirname(input), baseOf(input), fmt);
  return (await CONVERTERS[cat](input, out, fmt, opts)) || out;
}

module.exports = {
  convert,
  CATEGORIES,
  FALLBACK_TARGETS,
  AUDIO_ARGS,
  H264,
  extOf,
  baseOf,
  categoryOf,
  uniquePath,
  checkInput,
  withTmpDir,
  findSoffice,
  findTool,
  ffmpegTo,
  sharpInput,
  imagesToPdf,
  fodtToText,
};
