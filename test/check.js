// node --check untuk semua JS + script inline overlay.html, dan cek hash CSP.
// Pakai `node test/check.js --fix` untuk menulis ulang hash CSP setelah mengedit overlay.html.
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');

const root = path.join(__dirname, '..');
const jsFiles = ['main.js', 'preload.js', 'converter.js', 'tools.js', 'scripts/make-ico.js', 'test/converter.test.js', 'test/check.js'];
let ok = true;

for (const f of jsFiles) {
  try { execFileSync(process.execPath, ['--check', path.join(root, f)], { stdio: 'pipe' }); console.log(`OK    node --check ${f}`); }
  catch (e) { ok = false; console.log(`GAGAL node --check ${f}\n${e.stderr}`); }
}

const htmlPath = path.join(root, 'overlay.html');
// Browser menormalkan CRLF jadi LF sebelum menghitung hash CSP, jadi kita juga
const html = fs.readFileSync(htmlPath, 'utf8').replace(/\r\n?/g, '\n');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const style = html.match(/<style>([\s\S]*?)<\/style>/)[1];

const tmp = path.join(os.tmpdir(), 'shiftwheel-overlay-script.js');
fs.writeFileSync(tmp, script);
try { execFileSync(process.execPath, ['--check', tmp], { stdio: 'pipe' }); console.log('OK    node --check <script> overlay.html'); }
catch (e) { ok = false; console.log(`GAGAL node --check <script> overlay.html\n${e.stderr}`); }

const hash = (s) => `sha256-${crypto.createHash('sha256').update(s, 'utf8').digest('base64')}`;
const csp = `default-src 'none'; script-src '${hash(script)}'; style-src '${hash(style)}'`;
const cspRe = /(http-equiv="Content-Security-Policy" content=")([^"]*)(")/;
if (html.match(cspRe)[2] === csp) {
  console.log('OK    hash CSP cocok');
} else if (process.argv.includes('--fix')) {
  fs.writeFileSync(htmlPath, html.replace(cspRe, `$1${csp}$3`));
  console.log('FIX   hash CSP diperbarui');
} else {
  ok = false;
  console.log('GAGAL hash CSP tidak cocok, jalankan: node test/check.js --fix');
}

process.exit(ok ? 0 : 1);
