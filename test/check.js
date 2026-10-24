// node --check for every JS file + the inline scripts in overlay.html/hud.html, and verify the CSP hashes.
// Run `node test/check.js --fix` to rewrite the CSP hashes after editing overlay.html, hud.html, or crop.html.
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');

const root = path.join(__dirname, '..');
const jsFiles = ['main.js', 'preload.js', 'converter.js', 'tools.js', 'scripts/make-ico.js', 'scripts/dev.js', 'scripts/install-local.js', 'hotkeys.js', 'test/hotkeys.test.js', 'test/converter.test.js', 'test/check.js'];
let ok = true;

function nodeCheck(file, label) {
  try { execFileSync(process.execPath, ['--check', file], { stdio: 'pipe' }); console.log(`OK    node --check ${label}`); }
  catch (e) { ok = false; console.log(`FAIL  node --check ${label}\n${e.stderr}`); }
}

for (const f of jsFiles) nodeCheck(path.join(root, f), f);

const hash = (s) => `sha256-${crypto.createHash('sha256').update(s, 'utf8').digest('base64')}`;
const cspRe = /(http-equiv="Content-Security-Policy" content=")([^"]*)(")/;

for (const name of ['overlay.html', 'hud.html', 'crop.html']) {
  const htmlPath = path.join(root, name);
  // Browsers normalize CRLF to LF before hashing for CSP, so we do too
  const html = fs.readFileSync(htmlPath, 'utf8').replace(/\r\n?/g, '\n');
  const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
  const style = html.match(/<style>([\s\S]*?)<\/style>/)[1];

  const tmp = path.join(os.tmpdir(), `shiftwheel-${name}.js`);
  fs.writeFileSync(tmp, script);
  nodeCheck(tmp, `<script> ${name}`);

  // Only the hashes are rewritten; other directives (e.g. img-src in crop.html) are kept as written
  const current = html.match(cspRe)[2];
  const csp = current.replace(/script-src '[^']*'/, `script-src '${hash(script)}'`).replace(/style-src '[^']*'/, `style-src '${hash(style)}'`);
  if (/style="/.test(html)) {
    ok = false;
    console.log(`FAIL  ${name} has an inline style="" attribute, which the CSP blocks`);
  }
  if (current === csp) {
    console.log(`OK    CSP hashes in ${name} match`);
  } else if (process.argv.includes('--fix')) {
    fs.writeFileSync(htmlPath, html.replace(cspRe, `$1${csp}$3`));
    console.log(`FIX   CSP hashes in ${name} updated`);
  } else {
    ok = false;
    console.log(`FAIL  CSP hashes in ${name} don't match, run: node test/check.js --fix`);
  }
}

process.exit(ok ? 0 : 1);
