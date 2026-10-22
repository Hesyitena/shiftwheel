// Shiftwheel: proses utama (tray, hook global, overlay radial, antrean konversi)
const fs = require('fs');
const path = require('path');
const { app, BrowserWindow, Tray, Menu, Notification, screen, ipcMain, shell, powerMonitor } = require('electron');
const { uIOhook, UiohookKey } = require('uiohook-napi');
const { convert, FORMATS } = require('./converter');

const APP_ID = 'com.local.shiftwheel'; // harus sama dengan build.appId di package.json
const ICON = path.join(__dirname, 'assets', 'icon.png');
const OVERLAY_SIZE = 320;
const DRAG_THRESHOLD = 20; // piksel fisik
const HIDE_DELAY = 400; // beri waktu event drop sampai sebelum overlay hilang
const LINGER_MS = 8000; // mode --convert: tunggu sebentar supaya notifikasi masih bisa diklik
const LOGIN_ARGS = ['--hidden'];

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

// ---------- Antrean konversi (sequential) ----------

let queue = Promise.resolve();
let pending = 0;
let batch = newBatch();
let quitTimer = null;

function newBatch() {
  return { ok: 0, fail: 0, skip: 0, lastOut: null, note: null };
}

function enqueue(files, fmt) {
  clearTimeout(quitTimer);
  for (const file of files) {
    pending++;
    queue = queue.then(async () => {
      try {
        batch.lastOut = await convert(file, fmt);
        batch.ok++;
      } catch (err) {
        err.skipped ? batch.skip++ : batch.fail++;
        batch.note = batch.note || `${path.basename(String(file || ''))}: ${err.message}`;
      }
      if (--pending === 0) finishBatch();
    });
  }
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

// ---------- Overlay ----------

let shown = false;
let hideTimer = null;

function createOverlay() {
  overlay = new BrowserWindow({
    width: OVERLAY_SIZE,
    height: OVERLAY_SIZE,
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
  overlay.webContents.on('will-navigate', (e) => e.preventDefault());
  overlay.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  overlay.on('session-end', stopHook); // logoff / shutdown Windows
}

function showOverlay(physX, physY) {
  clearTimeout(hideTimer);
  // uiohook memberi piksel fisik; window butuh DIP (penting di scaling selain 100%)
  const p = screen.screenToDipPoint({ x: physX, y: physY });
  const wa = screen.getDisplayNearestPoint(p).workArea;
  const clamp = (v, min, max) => Math.min(Math.max(v, min), max);
  const x = clamp(Math.round(p.x - OVERLAY_SIZE / 2), wa.x, wa.x + wa.width - OVERLAY_SIZE);
  const y = clamp(Math.round(p.y - OVERLAY_SIZE / 2), wa.y, wa.y + wa.height - OVERLAY_SIZE);
  overlay.setBounds({ x, y, width: OVERLAY_SIZE, height: OVERLAY_SIZE });
  overlay.showInactive();
  overlay.webContents.send('overlay:show');
  shown = true;
}

function scheduleHide() {
  if (!shown) return;
  shown = false; // boleh muncul lagi kalau Shift ditekan ulang sebelum timer jalan
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => overlay && overlay.hide(), HIDE_DELAY);
}

ipcMain.on('overlay:drop', (e, msg) => {
  if (!overlay || e.sender !== overlay.webContents) return;
  const fmt = msg && msg.fmt;
  const paths = Array.isArray(msg && msg.paths) ? msg.paths.filter((p) => typeof p === 'string' && p) : [];
  if (!FORMATS.includes(fmt) || paths.length === 0) return; // drop di tengah / luar segmen = batal
  enqueue(paths, fmt);
});

// ---------- Hook global (uiohook) ----------
// Heuristik "file sedang di-drag": tombol kiri ditahan + gerak > 20 px + Shift.

const drag = { down: false, moved: false, shift: false, x0: 0, y0: 0, x: 0, y: 0 };
let hookRunning = false;

const isShiftKey = (e) => e.keycode === UiohookKey.Shift || e.keycode === UiohookKey.ShiftRight;

function maybeShow() {
  if (enabled && !shown && drag.down && drag.moved && drag.shift) showOverlay(drag.x, drag.y);
}

uIOhook.on('keydown', (e) => {
  if (!isShiftKey(e)) return;
  drag.shift = true;
  maybeShow();
});

uIOhook.on('keyup', (e) => {
  if (!isShiftKey(e)) return;
  drag.shift = false;
  scheduleHide();
});

uIOhook.on('mousedown', (e) => {
  if (e.button !== 1) return;
  Object.assign(drag, { down: true, moved: false, x0: e.x, y0: e.y, x: e.x, y: e.y });
});

uIOhook.on('mousemove', (e) => {
  if (!drag.down) return;
  drag.x = e.x;
  drag.y = e.y;
  if (!drag.moved && Math.hypot(e.x - drag.x0, e.y - drag.y0) > DRAG_THRESHOLD) drag.moved = true;
  maybeShow();
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
  Object.assign(drag, { down: false, moved: false, shift: false });
  scheduleHide();
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
  tray.setToolTip('Shiftwheel');
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

  if (firstRun && !hidden) notify('Shiftwheel berjalan di tray. Drag file + tahan Shift.');
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
