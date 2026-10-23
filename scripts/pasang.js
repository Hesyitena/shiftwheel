// npm run pasang: build installer lalu pasang diam-diam di PC ini. Versi lama yang sedang
// jalan ditutup oleh installer, versi baru langsung jalan di tray. Pengaturan tetap.
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');
const pkg = require('../package.json');

const root = path.join(__dirname, '..');
// Build di folder sementara, bukan dist/: dist/win-unpacked/resources/app.asar bisa terkunci
// oleh app Electron lain yang sedang membuka folder proyek (editor, Claude desktop, dll).
const out = path.join(os.tmpdir(), `shiftwheel-build-${Date.now()}`);
const node = (...args) => execFileSync(process.execPath, args, { cwd: root, stdio: 'inherit' });

node(path.join(root, 'scripts', 'make-ico.js'));
node(require.resolve('electron-builder/cli.js'), '--win', `--config.directories.output=${out}`);

const installer = path.join(out, `${pkg.build.productName} Setup ${pkg.version}.exe`);
console.log(`\nMemasang ${path.basename(installer)}...`);
// /S = tanpa jendela, --force-run = jalankan app setelah selesai (mode senyap tidak otomatis menjalankan)
execFileSync(installer, ['/S', '--force-run'], { stdio: 'inherit' });
try { fs.rmSync(out, { recursive: true, force: true }); } catch { /* sisa build di tmp, aman diabaikan */ }
console.log(`Shiftwheel ${pkg.version} terpasang dan sudah jalan di tray.`);
