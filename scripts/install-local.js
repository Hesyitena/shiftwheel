// npm run install-local: build the installer, then install it silently on this PC. The installer
// closes the running version and the new one starts in the tray right away. Settings are kept.
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const pkg = require('../package.json');

const root = path.join(__dirname, '..');
// Build into a fresh folder each time: a previous dist/win-unpacked/resources/app.asar can be locked by
// another Electron app that has the project folder open (an editor, Claude desktop, etc.). It stays on
// the project's drive rather than %TEMP%, so a full C: drive doesn't break the build.
const out = path.join(root, 'dist', `install-${Date.now()}`);
const node = (...args) => execFileSync(process.execPath, args, { cwd: root, stdio: 'inherit' });

node(path.join(root, 'scripts', 'make-ico.js'));
node(require.resolve('electron-builder/cli.js'), '--win', `--config.directories.output=${out}`);

const installer = path.join(out, `${pkg.build.productName} Setup ${pkg.version}.exe`);
console.log(`\nInstalling ${path.basename(installer)}...`);
// /S = no window, --force-run = start the app when done (silent mode doesn't start it by itself)
execFileSync(installer, ['/S', '--force-run'], { stdio: 'inherit' });
try { fs.rmSync(out, { recursive: true, force: true }); } catch { /* a locked leftover in dist/, safe to delete later */ }
console.log(`Shiftwheel ${pkg.version} is installed and running in the tray.`);
