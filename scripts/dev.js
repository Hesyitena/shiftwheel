// npm run dev: run Shiftwheel from source and restart it automatically whenever an app file changes.
const fs = require('fs');
const path = require('path');
const { spawn, execFileSync } = require('child_process');
const electron = require('electron'); // from Node, this is the path to electron.exe

const root = path.join(__dirname, '..');
const WATCHED = new Set(['main.js', 'preload.js', 'converter.js', 'tools.js', 'overlay.html', 'hud.html']);

// The installed version uses the same single-instance lock, so it has to be closed first
try {
  execFileSync('taskkill', ['/F', '/IM', 'Shiftwheel.exe'], { stdio: 'ignore' });
  console.log('[dev] Closed the installed Shiftwheel. Reopen it from the Start Menu when you are done.');
} catch { /* not running */ }

let child = null;
let timer = null;

function start() {
  child = spawn(electron, ['.'], { cwd: root, stdio: 'inherit' });
  // Quitting from the tray menu ends the dev session
  child.on('exit', (code) => process.exit(code || 0));
}

function restart(file) {
  console.log(`[dev] ${file} changed, restarting...`);
  child.removeAllListeners('exit');
  child.once('exit', start);
  child.kill();
}

// Watch the folder (not individual files): editors often save by replacing the file
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

console.log('[dev] Shiftwheel is running from source. Ctrl+C to stop.');
start();
