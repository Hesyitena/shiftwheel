// File conversion. No Electron dependency, so it can be tested with plain Node.
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFile, spawn } = require('child_process');
const { pathToFileURL } = require('url');
// sharp (libvips) and pdf-lib load on first use: ~17 MB of RAM we don't need while idle
let sharpLib = null;
function sharp(...args) {
  if (!sharpLib) {
    sharpLib = require('sharp');
    // The libvips cache keeps input files open; on Windows that means they can't be deleted or
    // moved while Shiftwheel runs. Each file is processed once, so the cache buys nothing.
    sharpLib.cache(false);
  }
  return sharpLib(...args);
}
const pdfLib = () => require('pdf-lib');

// File categories: source extensions and the target formats shown on the convert wheel
const CATEGORIES = {
  image: { name: 'Images', exts: ['png', 'jpg', 'webp', 'gif', 'tiff', 'avif', 'bmp', 'heic', 'svg'], targets: ['jpg', 'png', 'webp', 'avif', 'tiff', 'gif', 'bmp', 'pdf', 'txt'] },
  pdf: { name: 'PDFs', exts: ['pdf'], targets: ['docx', 'txt', 'jpg', 'png'] },
  document: { name: 'Documents', exts: ['doc', 'docx', 'odt', 'rtf', 'txt', 'html', 'md'], targets: ['pdf', 'docx', 'odt', 'rtf', 'txt', 'html'] },
  sheet: { name: 'Spreadsheets', exts: ['xls', 'xlsx', 'ods', 'csv'], targets: ['pdf', 'xlsx', 'ods', 'csv'] },
  slides: { name: 'Presentations', exts: ['ppt', 'pptx', 'odp'], targets: ['pdf', 'pptx', 'odp'] },
  audio: { name: 'Audio files', exts: ['mp3', 'm4a', 'wav', 'flac', 'ogg', 'opus', 'aiff', 'wma', 'aac'], targets: ['mp3', 'm4a', 'wav', 'flac', 'ogg', 'opus', 'aiff'] },
  video: { name: 'Videos', exts: ['mp4', 'mov', 'mkv', 'webm', 'avi', 'wmv', 'm4v'], targets: ['mp4', 'mov', 'mkv', 'webm', 'gif', 'mp3'] },
};
// Wheel shown when the file type is unknown (Windows doesn't always give a MIME type): common formats
const FALLBACK_TARGETS = ['pdf', 'docx', 'txt', 'jpg', 'png', 'mp3', 'mp4'];
const ALIAS = { jpeg: 'jpg', tif: 'tiff', htm: 'html', markdown: 'md', mdown: 'md', aif: 'aiff', heif: 'heic' };

// LibreOffice export filter per target format
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

// ffmpeg arguments per target format
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

// ---------- Utilities ----------

function extOf(file) {
  const e = path.extname(file).slice(1).toLowerCase();
  return ALIAS[e] || e;
}

const baseOf = (file) => path.basename(file, path.extname(file));

function categoryOf(file) {
  const ext = extOf(file);
  return Object.keys(CATEGORIES).find((k) => CATEGORIES[k].exts.includes(ext)) || null;
}

// Find an unused name: name.ext, name (1).ext, name (2).ext, ... (empty ext for folders)
function uniquePath(dir, base, ext) {
  const name = (b) => (ext ? `${b}.${ext}` : b);
  let candidate = path.join(dir, name(base));
  for (let i = 1; fs.existsSync(candidate); i++) candidate = path.join(dir, name(`${base} (${i})`));
  return candidate;
}

function run(cmd, args, timeout = 120000, env = process.env) {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, { timeout, env, windowsHide: true, maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) {
        err.stderr = String(stderr || '');
        return reject(err);
      }
      resolve(stdout);
    });
  });
}

const readdir = (dir) => { try { return fs.readdirSync(dir); } catch { return []; } };

// Find a helper program: common install folders, winget packages (not always on this process's PATH), then PATH
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

// ---------- OCR (built into Windows, offline) ----------

// Windows OCR only exists as a WinRT API, so it's called through PowerShell 5.1.
// Input: env SW_OCR = JSON array of image paths. Output: JSON array of texts (one per image).
// Exit 3 = no OCR language installed.
const OCR_SCRIPT = [
  "$ErrorActionPreference = 'Stop'",
  '[Console]::OutputEncoding = [Text.Encoding]::UTF8',
  'Add-Type -AssemblyName System.Runtime.WindowsRuntime',
  "$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]",
  'function Await($op, [Type]$type) { $t = $asTask.MakeGenericMethod($type).Invoke($null, @($op)); $null = $t.Wait(-1); $t.Result }',
  '$null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]',
  '$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]',
  '$null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics, ContentType = WindowsRuntime]',
  '$engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()',
  'if (-not $engine) { exit 3 }',
  '$texts = foreach ($path in (ConvertFrom-Json $env:SW_OCR)) {',
  '  $file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync($path)) ([Windows.Storage.StorageFile])',
  '  $stream = Await ($file.OpenAsync([Windows.Storage.FileAccessMode]::Read)) ([Windows.Storage.Streams.IRandomAccessStream])',
  '  try {',
  '    $decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])',
  '    $bitmap = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])',
  '    $result = Await ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])',
  "    ($result.Lines | ForEach-Object { $_.Text }) -join \"`n\"",
  '  } finally { $stream.Dispose() }',
  '}',
  'ConvertTo-Json -InputObject @($texts) -Compress',
].join('\n');

async function ocr(images) {
  const args = ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', Buffer.from(OCR_SCRIPT, 'utf16le').toString('base64')];
  let stdout;
  try {
    stdout = await run('powershell.exe', args, 300000, { ...process.env, SW_OCR: JSON.stringify(images) });
  } catch (err) {
    if (err.code === 3) {
      throw new Error('Windows OCR has no language installed. Add English (with the OCR feature) in Settings > Time & language > Language.');
    }
    throw new Error(`Windows OCR couldn't read the image. ${lastLine(err.stderr)}`.trim());
  }
  return JSON.parse(String(stdout).trim()).map((t) => String(t || '').trim());
}

// PDF pages without a text layer (scans/photos): render to PNG, then OCR
async function ocrPdf(input, tmp) {
  try {
    await run(findTool('pdftoppm'), ['-png', '-r', '200', input, path.join(tmp, 'ocr')]);
  } catch (err) {
    if (err.code === 'ENOENT') throw new Error('This PDF contains images, not text. Reading it (OCR) needs Poppler: winget install oschwartz10612.Poppler');
    throw new Error("pdftoppm couldn't read the PDF (damaged or password-protected).");
  }
  const pages = readdir(tmp).filter((f) => f.startsWith('ocr') && f.endsWith('.png')).sort().map((f) => path.join(tmp, f));
  return (await ocr(pages)).filter(Boolean).join('\n\n');
}

// ---------- ffmpeg ----------

async function probeDuration(input) {
  try {
    const out = await run(findTool('ffprobe'), ['-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', input]);
    return Number(String(out).trim()) || null;
  } catch {
    return null; // without a duration, progress shows as indeterminate
  }
}

// onProgress(0..1) is fed from ffmpeg's -progress report when the input duration is known
async function ffmpeg(args, onProgress) {
  const duration = onProgress ? await probeDuration(args[args.indexOf('-i') + 1]) : null;
  await new Promise((resolve, reject) => {
    // No timeout: long videos can take a while
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
      reject(err.code === 'ENOENT' ? new Error('ffmpeg not found. Install it first: winget install Gyan.FFmpeg') : err);
    });
    child.on('close', (code) => {
      if (code === 0) resolve();
      else reject(new Error(`ffmpeg couldn't process the file. ${lastLine(stderr)}`.trim()));
    });
  });
}

// Write to a temp folder first so a half-written file never stays behind on failure
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

// Plain text from a snippet of ODF XML
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

// Writer's PDF import puts each piece of text in its own frame, in no particular order,
// and TXT export ignores frames. So we read the text from FODT and sort it by position.
// ponytail: column/table layouts become rows read left->right; fine for ordinary text.
function fodtToText(xml) {
  const frames = [...xml.matchAll(/<draw:frame\b([^>]*)>([\s\S]*?)<\/draw:frame>/g)]
    .map(([, a, body]) => ({
      page: Number(attr(a, 'text:anchor-page-number')) || 0,
      x: toPt(attr(a, 'svg:x')),
      bottom: toPt(attr(a, 'svg:y')) + toPt(attr(a, 'svg:height')), // near the baseline, stable across font sizes
      // text boxes only; image frames carry base64 data that isn't text
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

// Convert Markdown (.md) to a styled standalone HTML document so LibreOffice imports headings, tables,
// code blocks, lists, and inline formatting as real document styles instead of raw plain text.
function mdToHtml(md, title = 'Document') {
  const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  const slots = [];
  const hold = (html) => `\x00S${slots.push(html) - 1}\x00`;
  const restore = (s) => s.replace(/\x00S(\d+)\x00/g, (_, i) => slots[Number(i)]);

  const inline = (raw) => {
    let s = String(raw).replace(/`([^`]+)`/g, (_, c) => hold(`<code>${esc(c)}</code>`));
    s = esc(s);
    s = s.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '<img alt="$1" src="$2">');
    s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');
    s = s.replace(/\*\*\*(.+?)\*\*\*/g, '<strong><em>$1</em></strong>');
    s = s.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
    s = s.replace(/__(.+?)__/g, '<strong>$1</strong>');
    s = s.replace(/\*(.+?)\*/g, '<em>$1</em>');
    s = s.replace(/(^|\W)_(.+?)_(?=\W|$)/g, '$1<em>$2</em>');
    s = s.replace(/~~(.+?)~~/g, '<s>$1</s>');
    return restore(s);
  };

  const lines = String(md || '').replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n').split('\n');
  const blocks = [];
  const splitRow = (r) => r.trim().replace(/^\||\|$/g, '').split('|').map((c) => c.trim());
  const isSep = (r) => /^\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)+\|?$/.test(r.trim());

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (!line.trim()) continue;

    const fence = /^(`{3,}|~{3,})/.exec(line.trim());
    if (fence) {
      const code = [];
      for (i++; i < lines.length && !lines[i].trim().startsWith(fence[1]); i++) code.push(esc(lines[i]));
      blocks.push(`<pre><code>${code.join('\n')}</code></pre>`);
      continue;
    }

    const h = /^(#{1,6})\s+(.+?)\s*#*$/.exec(line);
    if (h) {
      blocks.push(`<h${h[1].length}>${inline(h[2])}</h${h[1].length}>`);
      continue;
    }

    if (/^(?:-\s*){3,}$|^(?:\*\s*){3,}$|^(?:_\s*){3,}$/.test(line.trim())) {
      blocks.push('<hr>');
      continue;
    }

    if (line.includes('|') && i + 1 < lines.length && isSep(lines[i + 1])) {
      const head = splitRow(line);
      const aligns = splitRow(lines[++i]).map((c) => (c.startsWith(':') && c.endsWith(':') ? 'center' : c.endsWith(':') ? 'right' : 'left'));
      const rows = [];
      while (i + 1 < lines.length && lines[i + 1].includes('|') && lines[i + 1].trim()) rows.push(splitRow(lines[++i]));
      const th = head.map((c, j) => `<th align="${aligns[j] || 'left'}">${inline(c)}</th>`).join('');
      const tr = rows.map((r) => `<tr>${head.map((_, j) => `<td align="${aligns[j] || 'left'}">${inline(r[j] ?? '')}</td>`).join('')}</tr>`).join('');
      blocks.push(`<table><thead><tr>${th}</tr></thead><tbody>${tr}</tbody></table>`);
      continue;
    }

    if (/^\s*>\s?/.test(line)) {
      const quote = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) quote.push(inline(lines[i++].replace(/^\s*>\s?/, '')));
      i--;
      blocks.push(`<blockquote><p>${quote.join('<br>')}</p></blockquote>`);
      continue;
    }

    const listMatch = /^\s*([-*+]|\d+\.)\s+/.exec(line);
    if (listMatch) {
      const ordered = /\d+\./.test(listMatch[1]);
      const tag = ordered ? 'ol' : 'ul';
      const items = [];
      while (i < lines.length && (ordered ? /^\s*\d+\.\s+/ : /^\s*[-*+]\s+/).test(lines[i])) {
        const rawItem = lines[i++].replace(/^\s*(?:[-*+]|\d+\.)\s+/, '')
          .replace(/^\[x\]\s+/i, '&#9745; ').replace(/^\[\s\]\s+/, '&#9744; ');
        items.push(`<li>${inline(rawItem)}</li>`);
      }
      i--;
      blocks.push(`<${tag}>${items.join('')}</${tag}>`);
      continue;
    }

    const para = [inline(line.trim())];
    while (
      i + 1 < lines.length && lines[i + 1].trim() &&
      !/^(`{3,}|~{3,}|#{1,6}\s|>\s?|[-*+]\s|\d+\.\s|(?:-\s*){3,}$|(?:\*\s*){3,}$|(?:_\s*){3,}$)/.test(lines[i + 1].trim()) &&
      !(lines[i + 1].includes('|') && i + 2 < lines.length && isSep(lines[i + 2]))
    ) {
      para.push(inline(lines[++i].trim()));
    }
    blocks.push(`<p>${para.join(' ')}</p>`);
  }

  return `<!doctype html>
<html><head><meta charset="utf-8"><title>${esc(title)}</title>
<style>
  body { font-family: "Segoe UI", Arial, sans-serif; font-size: 11pt; line-height: 1.5; color: #111; }
  h1, h2, h3, h4, h5, h6 { margin: 1.1em 0 0.4em; line-height: 1.25; }
  h1 { font-size: 20pt; } h2 { font-size: 15pt; } h3 { font-size: 12.5pt; }
  p, ul, ol, blockquote, pre, table { margin: 0 0 0.8em; }
  code { font-family: Consolas, "Courier New", monospace; font-size: 9.5pt; background: #f3f3f5; padding: 1px 4px; }
  pre { font-family: Consolas, "Courier New", monospace; font-size: 9.5pt; background: #f3f3f5; padding: 10px; border: 1px solid #e1e1e6; white-space: pre-wrap; }
  pre code { background: none; padding: 0; }
  blockquote { border-left: 3px solid #ccc; padding-left: 10px; color: #555; }
  table { border-collapse: collapse; width: 100%; }
  th, td { border: 1px solid #ccc; padding: 6px 8px; }
  th { background: #f3f3f5; font-weight: bold; }
  a { color: #0066cc; }
</style></head><body>
${blocks.join('\n')}
</body></html>\n`;
}

async function viaLibreOffice(input, out, fmt) {
  const src = extOf(input);
  if (src === 'md' && fmt === 'html') {
    fs.writeFileSync(out, mdToHtml(fs.readFileSync(input, 'utf8'), baseOf(input)));
    return;
  }
  // Separate profile so it never clashes with a LibreOffice window the user has open
  const profile = pathToFileURL(path.join(os.tmpdir(), 'shiftwheel-lo-profile')).href;
  const infilter = src === 'pdf' ? 'writer_pdf_import' : src === 'csv' ? 'CSV:44,34,76' : null;
  const pdfText = src === 'pdf' && fmt === 'txt';
  const ext = pdfText ? 'fodt' : fmt;
  await withTmpDir(async (tmp) => {
    let loInput = input;
    if (src === 'md') {
      loInput = path.join(tmp, 'input.html');
      fs.writeFileSync(loInput, mdToHtml(fs.readFileSync(input, 'utf8'), baseOf(input)));
    }
    const args = [
      `-env:UserInstallation=${profile}`,
      '--headless', '--norestore', '--nolockcheck',
      ...(infilter ? [`--infilter=${infilter}`] : []),
      '--convert-to', pdfText ? 'fodt' : LO_FILTER[fmt],
      '--outdir', tmp,
      loInput,
    ];
    // Retry once: LibreOffice's first start (creating its profile) or a profile in use by another
    // process sometimes fails for no clear reason; the second attempt usually works.
    for (let attempt = 1; ; attempt++) {
      try {
        await run(findSoffice(), args);
      } catch (err) {
        if (err.code === 'ENOENT') throw new Error('LibreOffice not found. Install LibreOffice first.');
        if (err.killed) throw new Error('LibreOffice took too long to respond (over 2 minutes).');
        if (attempt < 2) continue;
        throw new Error(`LibreOffice couldn't convert the file. ${lastLine(err.stderr)}`.trim());
      }
      if (attempt >= 2 || readdir(tmp).some((f) => extOf(f) === ext)) break;
    }
    const produced = readdir(tmp).find((f) => extOf(f) === ext);
    if (!produced) throw new Error("LibreOffice didn't produce a file. Make sure the file isn't damaged or locked.");
    if (!pdfText) return fs.copyFileSync(path.join(tmp, produced), out);
    // PDFs without a text layer (scans/photos) are read with OCR.
    // ponytail: mixed PDFs (some scanned pages) only get text from pages that have a text layer.
    const text = fodtToText(fs.readFileSync(path.join(tmp, produced), 'utf8')) || await ocrPdf(input, tmp);
    if (!text) throw new Error('No readable text found in this PDF, not even with OCR.');
    fs.writeFileSync(out, text + '\n');
  });
}

// ---------- Images ----------

// sharp can't read BMP/HEIC: convert to PNG through ffmpeg first
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

// One or more images into one PDF, one page per image (595 pt wide)
async function imagesToPdf(files, out) {
  const doc = await pdfLib().PDFDocument.create();
  await withTmpDir(async (tmp) => {
    for (const file of files) {
      const isJpg = extOf(file) === 'jpg';
      const img = sharp(await sharpInput(file, tmp), { animated: false }).autoOrient();
      // pdf-lib can only embed JPG/PNG: JPG stays JPG, everything else becomes PNG (lossless)
      const { data, info } = await (isJpg ? img.jpeg({ quality: 92 }) : img.png()).toBuffer({ resolveWithObject: true });
      const embedded = isJpg ? await doc.embedJpg(data) : await doc.embedPng(data);
      const width = 595;
      const height = (width * info.height) / info.width;
      doc.addPage([width, height]).drawImage(embedded, { x: 0, y: 0, width, height });
    }
  });
  fs.writeFileSync(out, await doc.save());
}

async function imageToText(input, out) {
  await withTmpDir(async (tmp) => {
    const img = sharp(await sharpInput(input, tmp), { animated: false }).autoOrient();
    const { width = 0 } = (await img.metadata()).autoOrient || {};
    // Small text reads better when the image is upscaled; huge images get scaled down
    const resize = width && width < 1200 ? { width: width * 2 } : { width: 4000, height: 4000, fit: 'inside', withoutEnlargement: true };
    const png = path.join(tmp, 'ocr.png');
    await img.resize(resize).flatten({ background: '#ffffff' }).png().toFile(png);
    const [text] = await ocr([png]);
    if (!text) throw new Error('No readable text found in this image.');
    fs.writeFileSync(out, text + '\n');
  });
}

async function convertImage(input, out, fmt) {
  if (fmt === 'pdf') return imagesToPdf([input], out);
  if (fmt === 'txt') return imageToText(input, out);
  await withTmpDir(async (tmp) => {
    const src = await sharpInput(input, tmp);
    // GIF/WebP animation is kept when the target supports animation too
    const animated = ['gif', 'webp'].includes(fmt) && ['gif', 'webp'].includes(extOf(input));
    let img = sharp(src, { animated });
    if (!animated) img = img.autoOrient(); // follow EXIF orientation
    if (fmt === 'bmp') {
      const png = path.join(tmp, 'flat.png');
      await img.flatten({ background: '#ffffff' }).png().toFile(png);
      return ffmpegTo(png, [], out);
    }
    await ENCODE[fmt](img).toFile(out);
  });
}

// ---------- PDF to images ----------

// like: the file whose folder and name are used for the results (defaults to the PDF itself)
async function pdfToImages(input, out, fmt, like = input) {
  return withTmpDir(async (tmp) => {
    try {
      await run(findTool('pdftoppm'), [fmt === 'jpg' ? '-jpeg' : '-png', '-r', '150', input, path.join(tmp, 'p')]);
    } catch (err) {
      if (err.code === 'ENOENT') throw new Error('pdftoppm not found. Install Poppler first: winget install oschwartz10612.Poppler');
      throw new Error("pdftoppm couldn't read the PDF (damaged or password-protected).");
    }
    const pages = readdir(tmp).sort(); // p-01.jpg, p-02.jpg, ... (numbers are zero-padded)
    if (pages.length === 1) {
      fs.copyFileSync(path.join(tmp, pages[0]), out);
      return out;
    }
    // Several pages: "name 1.jpg", "name 2.jpg", ...
    const outs = pages.map((p, i) => {
      const dest = uniquePath(path.dirname(like), `${baseOf(like)} ${i + 1}`, fmt);
      fs.copyFileSync(path.join(tmp, p), dest);
      return dest;
    });
    return outs[0];
  });
}

// ---------- Main conversion ----------

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
    throw new Error(`File not found: ${path.basename(String(input || ''))}`);
  }
  if (!fs.statSync(input).isFile()) throw new Error("Folders can't be processed. Pick a file.");
}

/**
 * Convert one file to a target format. The result goes next to the source.
 * Returns the output path (the first page for multi-page PDFs).
 * An error with `skipped = true` means the file was skipped.
 * opts.onProgress(0..1) is called during audio/video conversion.
 */
async function convert(input, fmt, opts = {}) {
  fmt = ALIAS[String(fmt || '').toLowerCase()] || String(fmt || '').toLowerCase();
  const allTargets = new Set(Object.values(CATEGORIES).flatMap((c) => c.targets));
  if (!allTargets.has(fmt)) throw new Error(`Unknown target format: ${fmt}`);
  checkInput(input);

  const src = extOf(input);
  if (src === fmt) {
    const err = new Error(`Already a ${fmt.toUpperCase()} file`);
    err.skipped = true;
    throw err;
  }

  const cat = categoryOf(input);
  if (!cat) throw new Error(src ? `.${src} files aren't supported.` : "Files without an extension aren't supported.");
  if (!CATEGORIES[cat].targets.includes(fmt)) {
    throw new Error(`${CATEGORIES[cat].name} can't be converted to ${fmt.toUpperCase()}.`);
  }

  const out = uniquePath(path.dirname(input), baseOf(input), fmt);
  return (await CONVERTERS[cat](input, out, fmt, opts)) || out;
}

module.exports = {
  convert,
  sharp,
  pdfLib,
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
  ffmpeg,
  ffmpegTo,
  probeDuration,
  viaLibreOffice,
  pdfToImages,
  sharpInput,
  imagesToPdf,
  fodtToText,
};
