// npm run install-local: build the installer, then install it silently on this PC. The installer
// closes the running version and the new one starts in the tray right away. Settings are kept.
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');
const pkg = require('../package.json');

const root = path.join(__dirname, '..');
// Build into a temp folder, not dist/: dist/win-unpacked/resources/app.asar can be locked by
// another Electron app that has the project folder open (an editor, Claude desktop, etc.).
const out = path.join(os.tmpdir(), `shiftwheel-build-${Date.now()}`);
const node = (...args) => execFileSync(process.execPath, args, { cwd: root, stdio: 'inherit' });

node(path.join(root, 'scripts', 'make-ico.js'));
node(require.resolve('electron-builder/cli.js'), '--win', `--config.directories.output=${out}`);

const installer = path.join(out, `${pkg.build.productName} Setup ${pkg.version}.exe`);
console.log(`\nInstalling ${path.basename(installer)}...`);
// /S = no window, --force-run = start the app when done (silent mode doesn't start it by itself)
execFileSync(installer, ['/S', '--force-run'], { stdio: 'inherit' });
try { fs.rmSync(out, { recursive: true, force: true }); } catch { /* leftover build in tmp, safe to ignore */ }
console.log(`Shiftwheel ${pkg.version} is installed and running in the tray.`);
