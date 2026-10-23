// node --check untuk semua JS + script inline overlay.html/hud.html, dan cek hash CSP.
// Pakai `node test/check.js --fix` untuk menulis ulang hash CSP setelah mengedit overlay.html atau hud.html.
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');

const root = path.join(__dirname, '..');
const jsFiles = ['main.js', 'preload.js', 'converter.js', 'tools.js', 'scripts/make-ico.js', 'scripts/dev.js', 'scripts/pasang.js', 'test/converter.test.js', 'test/check.js'];
let ok = true;

function nodeCheck(file, label) {
  try { execFileSync(process.execPath, ['--check', file], { stdio: 'pipe' }); console.log(`OK    node --check ${label}`); }
  catch (e) { ok = false; console.log(`GAGAL node --check ${label}\n${e.stderr}`); }
}

for (const f of jsFiles) nodeCheck(path.join(root, f), f);

const hash = (s) => `sha256-${crypto.createHash('sha256').update(s, 'utf8').digest('base64')}`;
const cspRe = /(http-equiv="Content-Security-Policy" content=")([^"]*)(")/;

for (const name of ['overlay.html', 'hud.html']) {
  const htmlPath = path.join(root, name);
  // Browser menormalkan CRLF jadi LF sebelum menghitung hash CSP, jadi kita juga
  const html = fs.readFileSync(htmlPath, 'utf8').replace(/\r\n?/g, '\n');
  const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
  const style = html.match(/<style>([\s\S]*?)<\/style>/)[1];

  const tmp = path.join(os.tmpdir(), `shiftwheel-${name}.js`);
  fs.writeFileSync(tmp, script);
  nodeCheck(tmp, `<script> ${name}`);

  const csp = `default-src 'none'; script-src '${hash(script)}'; style-src '${hash(style)}'`;
  if (html.match(cspRe)[2] === csp) {
    console.log(`OK    hash CSP ${name} cocok`);
  } else if (process.argv.includes('--fix')) {
    fs.writeFileSync(htmlPath, html.replace(cspRe, `$1${csp}$3`));
    console.log(`FIX   hash CSP ${name} diperbarui`);
  } else {
    ok = false;
    console.log(`GAGAL hash CSP ${name} tidak cocok, jalankan: node test/check.js --fix`);
  }
}

process.exit(ok ? 0 : 1);
