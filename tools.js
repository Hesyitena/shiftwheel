// One-click tools for the Ctrl wheel. No Electron dependency, so it can be tested with plain Node.
const fs = require('fs');
const path = require('path');
const {
  sharp, pdfLib, AUDIO_ARGS, H264, extOf, baseOf, categoryOf, uniquePath, checkInput, withTmpDir, ffmpeg, ffmpegTo, probeDuration,
  sharpInput, imagesToPdf, viaLibreOffice, pdfToImages,
} = require('./converter');

// "photo.jpg" -> "photo (compressed).jpg" in the same folder
const suffixed = (file, label, ext = extOf(file)) => uniquePath(path.dirname(file), `${baseOf(file)} (${label})`, ext);

function skip(message) {
  const err = new Error(message);
  err.skipped = true;
  return err;
}

// ---------- Images ----------

// Formats sharp can write; BMP/HEIC/SVG are saved as PNG
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
    'compressed',
    (img) => img.resize({ width: 2560, height: 2560, fit: 'inside', withoutEnlargement: true }),
    { jpg: { quality: 75, mozjpeg: true }, png: { palette: true, quality: 80, compressionLevel: 9 }, webp: { quality: 70 }, avif: { quality: 45 }, tiff: { compression: 'jpeg', quality: 75 } }
  );
  if (fs.statSync(out).size >= fs.statSync(file).size) {
    fs.rmSync(out);
    throw skip("Already optimized, compressing didn't make it smaller");
  }
  return out;
}

function squareCrop(img, meta) {
  const side = Math.min(meta.autoOrient.width, meta.autoOrient.height);
  return img.resize(side, side, { fit: 'cover', position: 'attention' }); // 'attention': keep the most salient area
}

async function mergeImagesToPdf(files) {
  const out = uniquePath(path.dirname(files[0]), 'Merged', 'pdf');
  await imagesToPdf(files, out);
  return out;
}

// ---------- PDF ----------

async function loadPdf(file) {
  try {
    return await pdfLib().PDFDocument.load(fs.readFileSync(file), { updateMetadata: false });
  } catch {
    throw new Error("Couldn't read the PDF (damaged or password-protected).");
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
  const out = uniquePath(path.dirname(files[0]), 'Merged', 'pdf');
  fs.writeFileSync(out, await merged.save());
  return out;
}

async function splitPdf(file) {
  const doc = await loadPdf(file);
  const count = doc.getPageCount();
  if (count < 2) throw skip('The PDF has only 1 page');
  let first;
  for (let i = 0; i < count; i++) {
    const single = await pdfLib().PDFDocument.create();
    single.addPage((await single.copyPages(doc, [i]))[0]);
    const out = uniquePath(path.dirname(file), `${baseOf(file)} page ${i + 1}`, 'pdf');
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

// A small "1 / 5" centered at the bottom of each page.
// ponytail: rotated pages (/Rotate) get the number on the "bottom" of the unrotated page.
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
  if (doc.getPageCount() < 2) throw skip('The PDF has only 1 page');
  const out = await pdfLib().PDFDocument.create();
  for (const page of await out.copyPages(doc, doc.getPageIndices().reverse())) out.addPage(page);
  const dest = suffixed(file, 'reversed');
  fs.writeFileSync(dest, await out.save());
  return dest;
}

// ---------- Documents, spreadsheets, presentations (LibreOffice) ----------

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
  const out = uniquePath(path.dirname(files[0]), 'Merged', 'pdf');
  fs.writeFileSync(out, await merged.save());
  return out;
}

// Each page/slide becomes a PNG: "report 1.png", "report 2.png" (a single page: "report (page).png")
function docPages(file) {
  return withTmpDir(async (tmp) => {
    const pdf = path.join(tmp, 'doc.pdf');
    await viaLibreOffice(file, pdf, 'pdf');
    return pdfToImages(pdf, suffixed(file, 'page', 'png'), 'png', file);
  });
}

const DOC_TOOLS = (pagesLabel, pagesCaption) => [
  { id: 'merge', icon: 'merge', label: 'Merge PDF', caption: 'Merge everything into one PDF', multi: true, all: mergeDocsToPdf },
  { id: 'pages', icon: 'image', label: pagesLabel, caption: pagesCaption, each: docPages },
];

// ---------- Video & audio (ffmpeg) ----------

const H264_LIGHT = ['-c:v', 'libx264', '-preset', 'medium', '-crf', '28', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-b:a', '128k'];

function video(file, label, args, opts, ext = 'mp4') {
  const out = suffixed(file, label, ext);
  return ffmpegTo(file, args, out, opts.onProgress).then(() => out);
}

// Keep the source codec when we have one for it, otherwise MP3
function audio(file, label, args, opts, ext = AUDIO_ARGS[extOf(file)] ? extOf(file) : 'mp3') {
  const out = suffixed(file, label, ext);
  return ffmpegTo(file, ['-vn', ...args, ...AUDIO_ARGS[ext]], out, opts.onProgress).then(() => out);
}

// ffmpeg progress is measured against the input duration; scale it when the output is sped up so it still reaches 100%
const faster = (opts, factor) => ({ onProgress: opts.onProgress && ((p) => opts.onProgress(Math.min(1, p * factor))) });

// A frame from the middle of the video (-ss before -i: seek directly instead of decoding half the video)
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
  const d = Math.min(2, duration / 4); // short clips get a shorter fade
  const fade = `afade=t=in:d=${d.toFixed(2)},afade=t=out:st=${Math.max(0, duration - d).toFixed(2)}:d=${d.toFixed(2)}`;
  return audio(file, 'fade', ['-af', fade], opts);
}

// Remove leading silence, reverse, remove again (= trailing silence), reverse back
const TRIM_SILENCE = 'silenceremove=start_periods=1:start_threshold=-50dB,areverse,silenceremove=start_periods=1:start_threshold=-50dB,areverse';

// ---------- Tools per category ----------
// each(file, opts): runs per file. all(files): all files become one result (only shown when there are 2+ files).
// icon: icon name in overlay.html.

const TOOLS = {
  image: [
    { id: 'compress', icon: 'compress', label: 'Compress', caption: 'Make the image smaller', each: compressImage },
    { id: 'half', icon: 'half', label: '50%', caption: 'Scale down to 50%', each: (f) => sharpTool(f, '50%', (img, m) => img.resize(Math.max(1, Math.round(m.autoOrient.width / 2)))) },
    { id: 'rotate', icon: 'rotate', label: 'Rotate', caption: 'Rotate 90° clockwise', each: (f) => sharpTool(f, 'rotated', (img) => img.rotate(90)) },
    { id: 'gray', icon: 'gray', label: 'Grayscale', caption: 'Convert to grayscale', each: (f) => sharpTool(f, 'grayscale', (img) => img.grayscale()) },
    { id: 'strip', icon: 'strip', label: 'Metadata', caption: 'Remove metadata (EXIF, location)', each: (f) => sharpTool(f, 'no metadata', (img) => img) },
    { id: 'flip', icon: 'flip', label: 'Mirror', caption: 'Flip horizontally', each: (f) => sharpTool(f, 'mirrored', (img) => img.flop()) },
    { id: 'trim', icon: 'crop', label: 'Trim edges', caption: 'Cut away plain borders', each: (f) => sharpTool(f, 'trimmed', (img) => img.trim({ threshold: 12 })) },
    { id: 'square', icon: 'square', label: 'Square', caption: 'Crop to 1:1 around the most interesting part', each: (f) => sharpTool(f, 'square', squareCrop) },
    { id: 'merge', icon: 'merge', label: 'Merge PDF', caption: 'Combine into one PDF', multi: true, all: mergeImagesToPdf },
  ],
  pdf: [
    { id: 'merge', icon: 'merge', label: 'Merge', caption: 'Combine into one PDF', multi: true, all: mergePdfs },
    { id: 'split', icon: 'split', label: 'Split', caption: 'One PDF per page', each: splitPdf },
    { id: 'rotate', icon: 'rotate', label: 'Rotate', caption: 'Rotate every page 90°', each: (f) => pdfTool(f, 'rotated', (doc) => doc.getPages().forEach((p) => p.setRotation(pdfLib().degrees((p.getRotation().angle + 90) % 360)))) },
    { id: 'strip', icon: 'strip', label: 'Metadata', caption: 'Remove PDF metadata', each: (f) => pdfTool(f, 'no metadata', stripPdfMetadata) },
    { id: 'number', icon: 'hash', label: 'Page numbers', caption: 'Add page numbers at the bottom', each: (f) => pdfTool(f, 'numbered', numberPages) },
    { id: 'reverse', icon: 'reverse', label: 'Reverse', caption: 'Reverse the page order', each: reversePdf },
  ],
  video: [
    { id: 'compress', icon: 'compress', label: 'Compress', caption: 'Make the video smaller (max 1080p)', each: (f, o) => video(f, 'compressed', ['-vf', "scale=-2:'min(ih,1080)'", ...H264_LIGHT, '-movflags', '+faststart'], o) },
    { id: '720p', icon: 'screen', label: '720p', caption: 'Resize to 720p', each: (f, o) => video(f, '720p', ['-vf', 'scale=-2:720', ...H264, '-movflags', '+faststart'], o) },
    { id: 'mute', icon: 'mute', label: 'Mute', caption: 'Remove the audio', each: (f, o) => video(f, 'muted', ['-c:v', 'copy', '-an'], o, extOf(f)) },
    { id: 'rotate', icon: 'rotate', label: 'Rotate', caption: 'Rotate 90° clockwise', each: (f, o) => video(f, 'rotated', ['-vf', 'transpose=1', ...H264], o) },
    { id: 'thumb', icon: 'image', label: 'Thumbnail', caption: 'Grab a frame from the middle', each: videoThumb },
    { id: 'fast', icon: 'fast', label: '2× speed', caption: 'Play twice as fast', each: (f, o) => video(f, '2x', ['-vf', 'setpts=0.5*PTS', '-af', 'atempo=2', ...H264], faster(o, 2)) },
  ],
  audio: [
    { id: 'compress', icon: 'compress', label: 'Compress', caption: 'Compress to 128 kbps MP3', each: (f, o) => { const out = suffixed(f, 'compressed', 'mp3'); return ffmpegTo(f, ['-vn', '-c:a', 'libmp3lame', '-b:a', '128k'], out, o.onProgress).then(() => out); } },
    { id: 'normalize', icon: 'wave', label: 'Normalize', caption: 'Even out the loudness (loudnorm)', each: (f, o) => audio(f, 'normalized', ['-af', 'loudnorm=I=-16:TP=-1.5:LRA=11'], o) },
    { id: 'mono', icon: 'mono', label: 'Mono', caption: 'Mix down to mono', each: (f, o) => audio(f, 'mono', ['-ac', '1'], o) },
    { id: 'denoise', icon: 'denoise', label: 'Denoise', caption: 'Reduce hiss and background noise', each: (f, o) => audio(f, 'denoised', ['-af', 'afftdn=nf=-25'], o) },
    { id: 'trim', icon: 'scissors', label: 'Trim silence', caption: 'Cut silence at the start and end', each: (f, o) => audio(f, 'silence trimmed', ['-af', TRIM_SILENCE], o) },
    { id: 'fast', icon: 'fast', label: '1.5× speed', caption: 'Play 1.5× faster, same pitch', each: (f, o) => audio(f, '1.5x', ['-af', 'atempo=1.5'], faster(o, 1.5)) },
    { id: 'fade', icon: 'fade', label: 'Fade', caption: 'Fade in and out at the ends', each: fadeAudio },
  ],
  document: DOC_TOOLS('Pages', 'Save every page as a PNG'),
  sheet: DOC_TOOLS('Pages', 'Save every page as a PNG'),
  slides: DOC_TOOLS('Slides', 'Save every slide as a PNG'),
};

/**
 * Turn a tool choice ("image.compress") plus a list of files into queue tasks.
 * Each task: { name, label, chip, run: (opts) => Promise<output path> }.
 */
function toolTasks(toolId, files) {
  const [cat, id] = String(toolId).split('.');
  const tool = (TOOLS[cat] || []).find((t) => t.id === id);
  if (!tool) throw new Error(`Unknown tool: ${toolId}`);

  const ofCat = (f) => {
    checkInput(f);
    if (categoryOf(f) !== cat) throw new Error(`"${tool.caption}" doesn't work on .${extOf(f)} files`);
  };
  const info = { label: tool.caption, chip: tool.label };

  if (tool.all) {
    return [{
      ...info,
      name: `${files.length} files`,
      run: async () => {
        files.forEach(ofCat);
        if (files.length < 2) throw new Error('Pick at least 2 files to merge.');
        return tool.all(files);
      },
    }];
  }
  return files.map((f) => ({ ...info, name: path.basename(String(f || '')), run: async (opts = {}) => { ofCat(f); return tool.each(f, opts); } }));
}

module.exports = { TOOLS, toolTasks };
