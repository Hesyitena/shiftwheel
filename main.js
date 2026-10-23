// Shiftwheel: proses utama (tray, hook global, overlay radial, antrean konversi)
const fs = require('fs');
const path = require('path');
const { execFile, spawn } = require('child_process');
const { app, BrowserWindow, Tray, Menu, Notification, screen, ipcMain, shell, powerMonitor, clipboard, dialog } = require('electron');
const { uIOhook, UiohookKey } = require('uiohook-napi');
const { convert, uniquePath, CATEGORIES, FALLBACK_TARGETS } = require('./converter');
const { TOOLS, toolTasks } = require('./tools');

const APP_ID = 'com.local.shiftwheel'; // harus sama dengan build.appId di package.json
const ICON = path.join(__dirname, 'assets', 'icon.png');
const OVERLAY_W = 360;
const OVERLAY_H = 400;
const WHEEL_CENTER = 180; // pusat roda di dalam jendela overlay
const DRAG_THRESHOLD = 20; // piksel fisik
const HIDE_DELAY = 400; // beri waktu event drop sampai sebelum overlay hilang
const LINGER_MS = 8000; // mode --convert: tunggu sebentar supaya notifikasi masih bisa diklik
const LOGIN_ARGS = ['--hidden'];

// Ketuk tombol saat drag untuk membuka roda; ketuk tombol lain untuk ganti roda
const TRIGGER = {
  [UiohookKey.Shift]: 'convert',
  [UiohookKey.ShiftRight]: 'convert',
  [UiohookKey.Ctrl]: 'tools',
  [UiohookKey.CtrlRight]: 'tools',
  [UiohookKey.Alt]: 'actions',
  [UiohookKey.AltRight]: 'actions',
};

// Saat dev (npm start) belum ada shortcut Start Menu dengan AUMID ini, jadi toast
// diarahkan ke electron.exe supaya tetap muncul. Versi terinstal memakai APP_ID.
app.setAppUserModelId(app.isPackaged ? APP_ID : process.execPath);

const hidden = process.argv.includes('--hidden');
let convertOnly = parseConvert(process.argv) !== null;
let enabled = true;
let tray = null;
let overlay = null;

// ---------- Argumen --convert <format> <path> ----------

function parseConvert(argv) {
  const i = argv.indexOf('--convert');
  return i === -1 ? null : { fmt: String(argv[i + 1] || '').toLowerCase(), file: argv[i + 2] };
}

function handleArgv(argv) {
  const job = parseConvert(argv);
  if (job) enqueue([job.file], job.fmt);
  return job !== null;
}

// ---------- Pengaturan (userData/settings.json) ----------

const settingsFile = () => path.join(app.getPath('userData'), 'settings.json');

function loadSettings() {
  try { return JSON.parse(fs.readFileSync(settingsFile(), 'utf8')); } catch { return {}; }
}

function saveSettings(settings) {
  fs.mkdirSync(path.dirname(settingsFile()), { recursive: true });
  fs.writeFileSync(settingsFile(), JSON.stringify(settings, null, 2));
}

// ---------- Autostart ----------

function isAutostart() {
  const s = app.getLoginItemSettings({ args: LOGIN_ARGS });
  // executableWillLaunchAtLogin ikut memperhitungkan "Disabled" di Task Manager
  return s.executableWillLaunchAtLogin ?? s.openAtLogin;
}

function setAutostart(on) {
  if (!app.isPackaged) return; // jangan daftarkan electron.exe ke startup saat dev
  app.setLoginItemSettings({ openAtLogin: on, name: APP_ID, args: LOGIN_ARGS });
}

// ---------- Notifikasi ----------

const liveNotes = new Set(); // simpan referensi supaya event klik tidak hilang karena GC

function notify(body, onClick) {
  if (!Notification.isSupported()) return;
  const n = new Notification({ title: 'Shiftwheel', body });
  liveNotes.add(n);
  n.on('click', () => onClick && onClick());
  n.on('close', () => liveNotes.delete(n));
  n.show();
}

// ---------- Antrean (sequential) untuk konversi dan tools ----------

let queue = Promise.resolve();
let pending = 0;
let batch = newBatch();
let quitTimer = null;

function newBatch() {
  return { ok: 0, fail: 0, skip: 0, lastOut: null, note: null };
}

function updateProgress() {
  if (tray) tray.setToolTip(pending ? `Shiftwheel: memproses ${pending} file...` : 'Shiftwheel');
}

// task: { name, run: () => Promise<path hasil> }
function enqueueTasks(tasks) {
  clearTimeout(quitTimer);
  for (const task of tasks) {
    pending++;
    queue = queue.then(async () => {
      updateProgress();
      try {
        batch.lastOut = await task.run();
        batch.ok++;
      } catch (err) {
        if (err.skipped) batch.skip++;
        else batch.fail++;
        batch.note = batch.note || `${task.name}: ${err.message}`;
      }
      if (--pending === 0) finishBatch();
      updateProgress();
    });
  }
}

function enqueue(files, fmt) {
  enqueueTasks(files.map((f) => ({ name: path.basename(String(f || '')), run: () => convert(f, fmt) })));
}

function finishBatch() {
  const { ok, fail, skip, lastOut, note } = batch;
  batch = newBatch();
  let body = `${ok} berhasil, ${fail} gagal`;
  if (skip) body += `, ${skip} dilewati`;
  if (note && (fail || skip)) body += `\n${note}`;
  notify(body, lastOut ? () => shell.showItemInFolder(lastOut) : null);

  // Dipanggil dari menu klik kanan tanpa instance lain: keluar setelah antrean kosong
  if (convertOnly) quitTimer = setTimeout(() => { if (pending === 0) app.quit(); }, LINGER_MS);
}

// ---------- Aksi file (roda Alt) ----------

const count = (paths) => (paths.length === 1 ? `"${path.basename(paths[0])}"` : `${paths.length} item`);
const psQuote = (s) => `'${s.replace(/'/g, "''")}'`;

// Electron tidak bisa menaruh file (CF_HDROP) ke clipboard; PowerShell 5.1 bisa
function copyFilesToClipboard(paths) {
  const script = `Set-Clipboard -LiteralPath ${paths.map(psQuote).join(',')}`;
  const args = ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(script, 'utf16le').toString('base64')];
  return new Promise((resolve, reject) => {
    execFile('powershell.exe', args, { windowsHide: true }, (err) => (err ? reject(new Error('Gagal menyalin file ke clipboard.')) : resolve()));
  });
}

async function transfer(paths, move) {
  const { canceled, filePaths } = await dialog.showOpenDialog({
    title: move ? 'Pindahkan ke folder' : 'Salin ke folder',
    properties: ['openDirectory', 'createDirectory'],
  });
  if (canceled || !filePaths[0]) return null;
  const dir = filePaths[0];
  let last = null;
  for (const src of paths) {
    if (move && path.dirname(src) === dir) continue;
    const isFile = fs.statSync(src).isFile();
    const ext = isFile ? path.extname(src).slice(1) : '';
    const dest = uniquePath(dir, isFile ? path.basename(src, path.extname(src)) : path.basename(src), ext);
    if (move) {
      try {
        fs.renameSync(src, dest);
      } catch (err) {
        if (err.code !== 'EXDEV') throw err;
        fs.cpSync(src, dest, { recursive: true, errorOnExist: true, force: false }); // beda drive: salin lalu hapus
        fs.rmSync(src, { recursive: true });
      }
    } else {
      fs.cpSync(src, dest, { recursive: true, errorOnExist: true, force: false });
    }
    last = dest;
  }
  return { msg: `${count(paths)} ${move ? 'dipindah' : 'disalin'} ke ${dir}`, reveal: last };
}

function openTerminal(paths) {
  const p = paths[0];
  const dir = fs.statSync(p).isDirectory() ? p : path.dirname(p);
  const term = spawn('wt.exe', ['-d', dir], { detached: true, stdio: 'ignore' });
  term.on('error', () => spawn('cmd.exe', ['/K'], { cwd: dir, detached: true, stdio: 'ignore' }).unref()); // tanpa Windows Terminal
  term.unref();
}

const ACTIONS = [
  { id: 'copy-path', label: 'Salin Path', caption: 'Salin path lengkap', run: (p) => { clipboard.writeText(p.join('\r\n')); return { msg: `Path ${count(p)} disalin` }; } },
  { id: 'copy', label: 'Salin', caption: 'Salin ke clipboard (siap paste)', run: async (p) => { await copyFilesToClipboard(p); return { msg: `${count(p)} disalin ke clipboard` }; } },
  { id: 'copy-to', label: 'Salin Ke', caption: 'Salin ke folder lain...', run: (p) => transfer(p, false) },
  { id: 'move-to', label: 'Pindah Ke', caption: 'Pindahkan ke folder lain...', run: (p) => transfer(p, true) },
  { id: 'terminal', label: 'Terminal', caption: 'Buka folder di Terminal', run: openTerminal },
  { id: 'reveal', label: 'Explorer', caption: 'Tampilkan di Explorer', run: (p) => shell.showItemInFolder(p[0]) },
];

async function runAction(id, paths) {
  const action = ACTIONS.find((a) => a.id === id);
  if (!action) return;
  try {
    const existing = paths.filter((p) => fs.existsSync(p));
    if (!existing.length) throw new Error('File tidak ditemukan.');
    const result = await action.run(existing);
    if (result) notify(result.msg, result.reveal ? () => shell.showItemInFolder(result.reveal) : null);
  } catch (err) {
    notify(`${action.caption} gagal: ${err.message}`);
  }
}

// ---------- Isi roda (dikirim ke overlay) ----------

function wheelConfig() {
  const conv = (t) => ({ id: `convert:${t}`, label: t.toUpperCase(), caption: `Konversi ke ${t.toUpperCase()}` });
  const convert = { fallback: FALLBACK_TARGETS.map(conv) };
  for (const [cat, c] of Object.entries(CATEGORIES)) convert[cat] = c.targets.map(conv);
  const tools = {};
  for (const [cat, list] of Object.entries(TOOLS)) {
    tools[cat] = list.map((t) => ({ id: `tool:${cat}.${t.id}`, label: t.label, caption: t.caption, multi: Boolean(t.multi) }));
  }
  const actions = ACTIONS.map((a) => ({ id: `action:${a.id}`, label: a.label, caption: a.caption }));
  return { convert, tools, actions };
}

ipcMain.on('overlay:drop', (e, msg) => {
  if (!overlay || e.sender !== overlay.webContents) return;
  const id = msg && typeof msg.id === 'string' ? msg.id : '';
  const paths = Array.isArray(msg && msg.paths) ? msg.paths.filter((p) => typeof p === 'string' && p) : [];
  if (!id || paths.length === 0) return; // drop di tengah / luar segmen = batal

  const sep = id.indexOf(':');
  const kind = id.slice(0, sep);
  const value = id.slice(sep + 1);
  if (kind === 'convert') enqueue(paths, value);
  else if (kind === 'action') runAction(value, paths);
  else if (kind === 'tool') {
    try { enqueueTasks(toolTasks(value, paths)); } catch (err) { notify(err.message); }
  }
});

// ---------- Overlay ----------

let shown = false;
let hideTimer = null;

function createOverlay() {
  overlay = new BrowserWindow({
    width: OVERLAY_W,
    height: OVERLAY_H,
    show: false,
    frame: false,
    transparent: true,
    resizable: false,
    movable: false,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    skipTaskbar: true,
    focusable: false,
    hasShadow: false,
    alwaysOnTop: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      backgroundThrottling: false,
      spellcheck: false,
    },
  });
  overlay.setAlwaysOnTop(true, 'screen-saver');
  overlay.loadFile(path.join(__dirname, 'overlay.html'));
  overlay.webContents.on('did-finish-load', () => overlay.webContents.send('overlay:config', wheelConfig()));
  overlay.webContents.on('will-navigate', (e) => e.preventDefault());
  overlay.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  overlay.on('session-end', stopHook); // logoff / shutdown Windows
}

function placeOverlay(physX, physY) {
  // uiohook memberi piksel fisik; window butuh DIP (penting di scaling selain 100%)
  const p = screen.screenToDipPoint({ x: physX, y: physY });
  const wa = screen.getDisplayNearestPoint(p).workArea;
  const clamp = (v, min, max) => Math.min(Math.max(v, min), max);
  const x = clamp(Math.round(p.x - WHEEL_CENTER), wa.x, wa.x + wa.width - OVERLAY_W);
  const y = clamp(Math.round(p.y - WHEEL_CENTER), wa.y, wa.y + wa.height - OVERLAY_H);
  overlay.setBounds({ x, y, width: OVERLAY_W, height: OVERLAY_H });
}

function openWheel(mode) {
  const fresh = !shown;
  clearTimeout(hideTimer);
  if (fresh) placeOverlay(drag.x, drag.y);
  overlay.webContents.send('overlay:show', { mode, fresh });
  if (fresh) overlay.showInactive();
  shown = true;
}

function scheduleHide() {
  if (!shown) return;
  shown = false;
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => overlay && overlay.hide(), HIDE_DELAY);
}

function hideNow() {
  shown = false;
  clearTimeout(hideTimer);
  if (overlay) overlay.hide();
}

// ---------- Hook global (uiohook) ----------
// Heuristik "file sedang di-drag": tombol kiri ditahan + gerak > 20 px. Roda terbuka saat
// Shift/Ctrl/Alt diketuk selama drag dan tetap terbuka sampai mouse dilepas (atau Esc).

const drag = { down: false, moved: false, x0: 0, y0: 0, x: 0, y: 0 };
const held = new Set(); // abaikan auto-repeat saat tombol ditahan
let hookRunning = false;

uIOhook.on('keydown', (e) => {
  if (e.keycode === UiohookKey.Escape) return hideNow();
  const mode = TRIGGER[e.keycode];
  if (!mode || held.has(e.keycode)) return;
  held.add(e.keycode);
  if (enabled && drag.down && drag.moved) openWheel(mode);
});

uIOhook.on('keyup', (e) => held.delete(e.keycode));

uIOhook.on('mousedown', (e) => {
  if (e.button !== 1) return;
  Object.assign(drag, { down: true, moved: false, x0: e.x, y0: e.y, x: e.x, y: e.y });
});

uIOhook.on('mousemove', (e) => {
  if (!drag.down) return;
  drag.x = e.x;
  drag.y = e.y;
  if (!drag.moved && Math.hypot(e.x - drag.x0, e.y - drag.y0) > DRAG_THRESHOLD) drag.moved = true;
});

uIOhook.on('mouseup', (e) => {
  if (e.button !== 1) return;
  drag.down = false;
  scheduleHide();
});

function startHook() {
  if (hookRunning || !enabled) return;
  try {
    uIOhook.start();
    hookRunning = true;
  } catch (err) {
    notify(`Hook global gagal dijalankan: ${err.message}`);
  }
}

function stopHook() {
  if (!hookRunning) return;
  try { uIOhook.stop(); } catch { /* sudah berhenti */ }
  hookRunning = false;
  held.clear();
  Object.assign(drag, { down: false, moved: false });
  hideNow();
}

// ---------- Tray ----------

function buildMenu() {
  return Menu.buildFromTemplate([
    {
      label: 'Aktif',
      type: 'checkbox',
      checked: enabled,
      click: (item) => {
        enabled = item.checked;
        enabled ? startHook() : stopHook();
      },
    },
    {
      label: 'Jalankan saat Windows menyala',
      type: 'checkbox',
      checked: app.isPackaged && isAutostart(),
      enabled: app.isPackaged, // nonaktif saat dev
      click: (item) => setAutostart(item.checked),
    },
    { type: 'separator' },
    { label: 'Keluar', click: () => app.quit() },
  ]);
}

function createTray() {
  tray = new Tray(ICON);
  updateProgress();
  // Menu dibangun ulang tiap dibuka supaya status autostart selalu sesuai registry
  const open = () => tray.popUpContextMenu(buildMenu());
  tray.on('click', open);
  tray.on('right-click', open);
}

// ---------- Mode latar belakang ----------

function startBackground() {
  convertOnly = false;
  clearTimeout(quitTimer);

  const settings = loadSettings();
  const firstRun = settings.firstRun !== false;
  if (firstRun) {
    setAutostart(true);
    saveSettings({ ...settings, firstRun: false });
  }

  createOverlay();
  createTray();
  startHook();

  powerMonitor.on('suspend', stopHook);
  powerMonitor.on('resume', () => { stopHook(); startHook(); }); // hook bisa mati setelah sleep

  if (firstRun && !hidden) notify('Shiftwheel berjalan di tray. Drag file lalu ketuk Shift (konversi), Ctrl (tools), atau Alt (aksi).');
}

// ---------- Single instance ----------

if (!app.requestSingleInstanceLock({ argv: process.argv })) {
  app.quit();
} else {
  // Pakai additionalData: argv bawaan event ini bisa berubah urutan di Windows
  app.on('second-instance', (_e, _argv, _cwd, data) => {
    const argv = (data && data.argv) || _argv;
    if (handleArgv(argv)) return;
    if (convertOnly) return startBackground(); // app dibuka normal saat instance --convert masih jalan
    if (!argv.includes('--hidden')) notify('Sudah berjalan di tray');
  });

  app.whenReady().then(() => {
    if (convertOnly) handleArgv(process.argv);
    else startBackground();
  });

  app.on('will-quit', stopHook);
}
