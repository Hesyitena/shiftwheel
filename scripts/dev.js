// npm run dev: jalankan Shiftwheel dari source dan restart otomatis setiap file app berubah.
const fs = require('fs');
const path = require('path');
const { spawn, execFileSync } = require('child_process');
const electron = require('electron'); // dari Node, ini path ke electron.exe

const root = path.join(__dirname, '..');
const WATCHED = new Set(['main.js', 'preload.js', 'converter.js', 'tools.js', 'overlay.html', 'hud.html']);

// Versi terinstal memakai kunci single-instance yang sama, jadi harus ditutup dulu
try {
  execFileSync('taskkill', ['/F', '/IM', 'Shiftwheel.exe'], { stdio: 'ignore' });
  console.log('[dev] Shiftwheel terinstal ditutup. Buka lagi dari Start Menu setelah selesai.');
} catch { /* tidak sedang jalan */ }

let child = null;
let timer = null;

function start() {
  child = spawn(electron, ['.'], { cwd: root, stdio: 'inherit' });
  // Keluar dari menu tray = selesai dev
  child.on('exit', (code) => process.exit(code || 0));
}

function restart(file) {
  console.log(`[dev] ${file} berubah, restart...`);
  child.removeAllListeners('exit');
  child.once('exit', start);
  child.kill();
}

// Pantau foldernya (bukan file satu-satu): editor sering menyimpan dengan cara mengganti file
fs.watch(root, (_event, file) => {
  if (!WATCHED.has(file)) return;
  clearTimeout(timer);
  timer = setTimeout(() => restart(file), 250);
});

process.on('SIGINT', () => {
  child.removeAllListeners('exit');
  child.kill();
  process.exit(0);
});

console.log('[dev] Shiftwheel jalan dari source. Ctrl+C untuk berhenti.');
start();
