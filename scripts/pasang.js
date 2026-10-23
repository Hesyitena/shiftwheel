// npm run pasang: build installer lalu pasang diam-diam di PC ini. Versi lama yang sedang
// jalan ditutup oleh installer, versi baru langsung jalan di tray. Pengaturan tetap.
const path = require('path');
const { execSync, execFileSync } = require('child_process');
const pkg = require('../package.json');

const root = path.join(__dirname, '..');
execSync('npm run dist', { cwd: root, stdio: 'inherit' });

const installer = path.join(root, 'dist', `${pkg.build.productName} Setup ${pkg.version}.exe`);
console.log(`\nMemasang ${path.basename(installer)}...`);
// /S = tanpa jendela, --force-run = jalankan app setelah selesai (mode senyap tidak otomatis menjalankan)
execFileSync(installer, ['/S', '--force-run'], { stdio: 'inherit' });
console.log(`Shiftwheel ${pkg.version} terpasang dan sudah jalan di tray.`);
