// Shiftwheel: main process (tray, global hook, radial overlay, conversion queue)
const fs = require('fs');
const path = require('path');
const { execFile, spawn } = require('child_process');
const { app, BrowserWindow, Tray, Menu, Notification, nativeImage, screen, ipcMain, shell, powerMonitor, clipboard, dialog } = require('electron');
const { uIOhook, UiohookKey } = require('uiohook-napi');
const { convert, uniquePath, CATEGORIES, FALLBACK_TARGETS } = require('./converter');
const { TOOLS, toolTasks } = require('./tools');

const APP_ID = 'com.local.shiftwheel'; // must match build.appId in package.json
const OVERLAY_W = 360;
const OVERLAY_H = 380;
const HUD_W = 340;
const HUD_H = 92;
const HUD_DELAY = 600; // ms before the progress HUD appears
const WHEEL_CENTER = 180; // wheel center inside the overlay window
const DRAG_THRESHOLD = 16; // DIP (~20 physical px at 125% scaling)
const HIDE_DELAY = 400; // give the drop event time to arrive before the overlay hides
const LINGER_MS = 8000; // --convert mode: wait a little so the notification can still be clicked
const LOGIN_ARGS = ['--hidden'];

// Tap a key while dragging to open a wheel; tap another key to switch wheels
const TRIGGER = {
  [UiohookKey.Shift]: 'convert',
  [UiohookKey.ShiftRight]: 'convert',
  [UiohookKey.Ctrl]: 'tools',
  [UiohookKey.CtrlRight]: 'tools',
  [UiohookKey.Alt]: 'actions',
  [UiohookKey.AltRight]: 'actions',
};

// Save resources: a simple wheel renders fine on the CPU (GPU process ~80 MB -> ~15 MB, transparency
// still works), and the network service runs in the main process since Shiftwheel never uses the network.
app.disableHardwareAcceleration();
app.commandLine.appendSwitch('enable-features', 'NetworkServiceInProcess2');

// In dev (npm start) there's no Start Menu shortcut with this AUMID yet, so toasts are
// attributed to electron.exe so they still show. The installed app uses APP_ID.
app.setAppUserModelId(app.isPackaged ? APP_ID : process.execPath);

const hidden = process.argv.includes('--hidden');
let convertOnly = parseConvert(process.argv) !== null;
let enabled = true;
let soundOn = true; // from settings.json, toggled from the tray menu
let tray = null;
let overlay = null;

// ---------- --convert <format> <path> argument ----------

function parseConvert(argv) {
  const i = argv.indexOf('--convert');
  return i === -1 ? null : { fmt: String(argv[i + 1] || '').toLowerCase(), file: argv[i + 2] };
}

function handleArgv(argv) {
  const job = parseConvert(argv);
  if (job) enqueue([job.file], job.fmt);
  return job !== null;
}

// ---------- Settings (userData/settings.json) ----------

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
  // executableWillLaunchAtLogin also respects "Disabled" in Task Manager
  return s.executableWillLaunchAtLogin ?? s.openAtLogin;
}

function setAutostart(on) {
  if (!app.isPackaged) return; // never register electron.exe at startup during dev
  app.setLoginItemSettings({ openAtLogin: on, name: APP_ID, args: LOGIN_ARGS });
}

// ---------- Notifications ----------

const liveNotes = new Set(); // keep references so click events aren't lost to GC

function notify(title, body, onClick) {
  if (!Notification.isSupported()) return;
  const n = new Notification({ title, body: body || '' });
  liveNotes.add(n);
  n.on('click', () => onClick && onClick());
  n.on('close', () => liveNotes.delete(n));
  n.show();
}

// ---------- Queue (sequential) for conversions and tools ----------

let queue = Promise.resolve();
let pending = 0;
let batch = newBatch();
let quitTimer = null;

function newBatch() {
  return { ok: 0, fail: 0, skip: 0, done: 0, total: 0, lastOut: null, note: null, label: null, started: 0 };
}

function updateProgress() {
  if (tray) tray.setToolTip(pending ? `Shiftwheel: processing ${pending} ${pending === 1 ? 'file' : 'files'}...` : 'Shiftwheel');
}

// task: { name, label, chip, run: (opts) => Promise<output path> }
function enqueueTasks(tasks) {
  clearTimeout(quitTimer);
  for (const task of tasks) {
    pending++;
    batch.total++;
    batch.started = batch.started || Date.now();
    batch.label = batch.label === null || batch.label === task.label ? task.label : 'Done';
    queue = queue.then(async () => {
      updateProgress();
      const info = { name: task.name, label: task.label, chip: task.chip, index: batch.done + 1, total: batch.total, percent: null };
      // The HUD only appears when a task takes a while, so quick conversions don't flash it
      if (hudVisible) hudUpdate(info);
      else hudTimer = setTimeout(() => hudUpdate(info), HUD_DELAY);
      const onProgress = (p) => { info.percent = p; if (hudVisible) hudUpdate(info); };
      try {
        batch.lastOut = await task.run({ onProgress });
        batch.ok++;
      } catch (err) {
        if (err.skipped) batch.skip++;
        else batch.fail++;
        batch.note = batch.note || `${task.name}: ${err.message}`;
      }
      clearTimeout(hudTimer);
      batch.done++;
      if (--pending === 0) {
        hudHide();
        finishBatch();
      }
      updateProgress();
    });
  }
}

function enqueue(files, fmt) {
  const label = `Convert to ${fmt.toUpperCase()}`;
  enqueueTasks(files.map((f) => ({
    name: path.basename(String(f || '')), label, chip: fmt.toUpperCase(), run: (opts) => convert(f, fmt, opts),
  })));
}

const files = (n) => `${n} ${n === 1 ? 'file' : 'files'}`;

function finishBatch() {
  const { ok, fail, skip, total, lastOut, note, label, started } = batch;
  batch = newBatch();
  const open = lastOut ? () => shell.showItemInFolder(lastOut) : null;
  // The "done" chime only plays for slow jobs; instant ones already got the "select" sound on drop
  if (fail) playSound('error');
  else if (ok && Date.now() - started > 1500) playSound('done');
  if (!fail && !skip) {
    notify(ok === 1 ? `${path.basename(lastOut)} is ready` : `${files(ok)} done`, `${label}. Click to open the folder.`, open);
  } else if (!ok && !fail) {
    notify(total === 1 ? 'Skipped' : `${files(total)} skipped`, note);
  } else if (!ok) {
    notify(total === 1 ? "Couldn't process the file" : `${files(total)} failed`, note);
  } else {
    notify(`${ok} of ${files(total)} done`, `${fail + skip} not processed. ${note}`, open);
  }

  // Started from the Explorer context menu with no other instance: quit once the queue is empty
  if (convertOnly) quitTimer = setTimeout(() => { if (pending === 0) app.quit(); }, LINGER_MS);
}

// ---------- Progress HUD (bottom-right corner) ----------

let hud = null;
let hudVisible = false;
let hudTimer = null;
let hudLastSend = 0;

function floatingWindow(width, height) {
  const win = new BrowserWindow({
    width,
    height,
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
      autoplayPolicy: 'no-user-gesture-required', // overlay sounds without a click first
    },
  });
  win.setAlwaysOnTop(true, 'screen-saver');
  win.webContents.on('will-navigate', (e) => e.preventDefault());
  win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  return win;
}

function hudUpdate(info) {
  if (!hud) {
    hud = floatingWindow(HUD_W, HUD_H);
    hud.loadFile(path.join(__dirname, 'hud.html'));
    hud.webContents.once('did-finish-load', () => hudVisible && hud.webContents.send('hud:update', info));
  }
  if (!hudVisible) {
    const wa = screen.getDisplayNearestPoint(screen.getCursorScreenPoint()).workArea;
    hud.setBounds({ x: wa.x + wa.width - HUD_W - 8, y: wa.y + wa.height - HUD_H - 4, width: HUD_W, height: HUD_H });
    hud.showInactive();
    hudVisible = true;
  } else if (Date.now() - hudLastSend < 100 && info.percent !== 1) {
    return; // throttle progress updates to ~10 per second
  }
  hudLastSend = Date.now();
  if (!hud.webContents.isLoading()) hud.webContents.send('hud:update', info);
}

function hudHide() {
  clearTimeout(hudTimer);
  if (!hudVisible) return;
  hudVisible = false;
  hud.webContents.send('hud:hide');
  // Destroy after the fade: the HUD renderer shouldn't sit idle in memory (it's recreated when needed)
  setTimeout(() => {
    if (hudVisible || !hud) return;
    hud.destroy();
    hud = null;
  }, 250);
}

// ---------- File actions (Alt wheel) ----------

const count = (paths) => (paths.length === 1 ? `"${path.basename(paths[0])}"` : `${paths.length} items`);
const psQuote = (s) => `'${s.replace(/'/g, "''")}'`;

// Electron can't put files (CF_HDROP) on the clipboard; PowerShell 5.1 can
function copyFilesToClipboard(paths) {
  const script = `Set-Clipboard -LiteralPath ${paths.map(psQuote).join(',')}`;
  const args = ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(script, 'utf16le').toString('base64')];
  return new Promise((resolve, reject) => {
    execFile('powershell.exe', args, { windowsHide: true }, (err) => (err ? reject(new Error("Couldn't copy the files to the clipboard.")) : resolve()));
  });
}

async function transfer(paths, move) {
  const { canceled, filePaths } = await dialog.showOpenDialog({
    title: move ? 'Move to folder' : 'Copy to folder',
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
        fs.cpSync(src, dest, { recursive: true, errorOnExist: true, force: false }); // different drive: copy, then delete
        fs.rmSync(src, { recursive: true });
      }
    } else {
      fs.cpSync(src, dest, { recursive: true, errorOnExist: true, force: false });
    }
    last = dest;
  }
  return { msg: `${count(paths)} ${move ? 'moved' : 'copied'} to ${dir}`, reveal: last };
}

function openTerminal(paths) {
  const p = paths[0];
  const dir = fs.statSync(p).isDirectory() ? p : path.dirname(p);
  const term = spawn('wt.exe', ['-d', dir], { detached: true, stdio: 'ignore' });
  term.on('error', () => spawn('cmd.exe', ['/K'], { cwd: dir, detached: true, stdio: 'ignore' }).unref()); // no Windows Terminal
  term.unref();
}

const ACTIONS = [
  { id: 'copy-path', icon: 'link', label: 'Copy path', caption: 'Copy the full path as text', run: (p) => { clipboard.writeText(p.join('\r\n')); return { msg: `Copied the path of ${count(p)}` }; } },
  { id: 'copy', icon: 'copy', label: 'Copy', caption: 'Copy to the clipboard, ready to paste', run: async (p) => { await copyFilesToClipboard(p); return { msg: `${count(p)} ready to paste` }; } },
  { id: 'copy-to', icon: 'copy-to', label: 'Copy to', caption: 'Copy to another folder', run: (p) => transfer(p, false) },
  { id: 'move-to', icon: 'move-to', label: 'Move to', caption: 'Move to another folder', run: (p) => transfer(p, true) },
  { id: 'terminal', icon: 'terminal', label: 'Terminal', caption: 'Open the folder in Terminal', run: openTerminal },
  { id: 'reveal', icon: 'folder', label: 'Explorer', caption: 'Show in Explorer', run: (p) => shell.showItemInFolder(p[0]) },
];

async function runAction(id, paths) {
  const action = ACTIONS.find((a) => a.id === id);
  if (!action) return;
  try {
    const existing = paths.filter((p) => fs.existsSync(p));
    if (!existing.length) throw new Error('File not found.');
    const result = await action.run(existing);
    if (result) notify(result.msg, result.reveal ? 'Click to open the folder.' : '', result.reveal ? () => shell.showItemInFolder(result.reveal) : null);
  } catch (err) {
    playSound('error');
    notify(`${action.label} failed`, err.message);
  }
}

// ---------- Wheel contents (sent to the overlay) ----------

function wheelConfig() {
  const conv = (t) => ({ id: `convert:${t}`, label: t.toUpperCase(), caption: `Convert to ${t.toUpperCase()}` });
  const convert = { fallback: FALLBACK_TARGETS.map(conv) };
  for (const [cat, c] of Object.entries(CATEGORIES)) convert[cat] = c.targets.map(conv);
  convert.image.find((i) => i.id === 'convert:txt').caption = 'Extract text from the image (OCR)';
  const tools = {};
  for (const [cat, list] of Object.entries(TOOLS)) {
    tools[cat] = list.map((t) => ({ id: `tool:${cat}.${t.id}`, label: t.label, caption: t.caption, icon: t.icon, multi: Boolean(t.multi) }));
  }
  const actions = ACTIONS.map((a) => ({ id: `action:${a.id}`, label: a.label, caption: a.caption, icon: a.icon }));
  return { convert, tools, actions };
}

ipcMain.on('overlay:drop', (e, msg) => {
  if (!overlay || e.sender !== overlay.webContents) return;
  const id = msg && typeof msg.id === 'string' ? msg.id : '';
  const paths = Array.isArray(msg && msg.paths) ? msg.paths.filter((p) => typeof p === 'string' && p) : [];
  if (!id || paths.length === 0) return; // dropped in the center / outside a segment = cancel

  const sep = id.indexOf(':');
  const kind = id.slice(0, sep);
  const value = id.slice(sep + 1);
  if (kind === 'convert') enqueue(paths, value);
  else if (kind === 'action') runAction(value, paths);
  else if (kind === 'tool') {
    try { enqueueTasks(toolTasks(value, paths)); } catch (err) { notify("Can't use this tool", err.message); }
  }
});

// ---------- Overlay ----------

let shown = false;
let hideTimer = null;

function createOverlay() {
  overlay = floatingWindow(OVERLAY_W, OVERLAY_H);
  overlay.loadFile(path.join(__dirname, 'overlay.html'));
  overlay.webContents.on('did-finish-load', () => {
    overlay.webContents.send('overlay:config', wheelConfig());
    overlay.webContents.send('overlay:settings', { sound: soundOn });
  });
  overlay.on('session-end', stopHook); // Windows logoff / shutdown
}

function placeOverlay(p) {
  const wa = screen.getDisplayNearestPoint(p).workArea;
  const clamp = (v, min, max) => Math.min(Math.max(v, min), max);
  const x = clamp(Math.round(p.x - WHEEL_CENTER), wa.x, wa.x + wa.width - OVERLAY_W);
  const y = clamp(Math.round(p.y - WHEEL_CENTER), wa.y, wa.y + wa.height - OVERLAY_H);
  overlay.setBounds({ x, y, width: OVERLAY_W, height: OVERLAY_H });
}

function openWheel(mode, cursor) {
  const fresh = !shown;
  clearTimeout(hideTimer);
  if (fresh) placeOverlay(cursor);
  overlay.webContents.send('overlay:show', { mode, fresh });
  if (fresh) overlay.showInactive();
  shown = true;
}

function scheduleHide() {
  if (!shown) return;
  shown = false;
  clearTimeout(hideTimer);
  overlay.webContents.send('overlay:close'); // fade-out animation
  hideTimer = setTimeout(() => overlay && overlay.hide(), HIDE_DELAY);
}

// Sounds play in the overlay window (the main process has no audio)
function playSound(name) {
  if (soundOn && overlay) overlay.webContents.send('overlay:sound', name);
}

function hideNow() {
  shown = false;
  clearTimeout(hideTimer);
  if (overlay) overlay.hide();
}

// ---------- Global hook (uiohook) ----------
// "A file is being dragged" heuristic: left button held and the cursor has moved > DRAG_THRESHOLD
// when Shift/Ctrl/Alt is tapped. The wheel stays open until the mouse is released (or Esc).
// Deliberately no mousemove listener: the distance is checked once when a key is tapped, so
// everyday mouse movement costs almost no CPU.

const drag = { down: false, x0: 0, y0: 0 }; // click position in DIP
const held = new Set(); // ignore auto-repeat while a key is held
let hookRunning = false;

uIOhook.on('keydown', (e) => {
  if (e.keycode === UiohookKey.Escape) {
    if (shown) playSound('cancel');
    return hideNow();
  }
  const mode = TRIGGER[e.keycode];
  if (!mode || held.has(e.keycode)) return;
  held.add(e.keycode);
  if (!enabled || !drag.down) return;
  if (shown) return openWheel(mode); // switch wheels
  const cursor = screen.getCursorScreenPoint();
  if (Math.hypot(cursor.x - drag.x0, cursor.y - drag.y0) > DRAG_THRESHOLD) openWheel(mode, cursor);
});

uIOhook.on('keyup', (e) => held.delete(e.keycode));

uIOhook.on('mousedown', (e) => {
  if (e.button !== 1) return;
  // uiohook reports physical pixels; convert to DIP so it's right at scaling other than 100%
  const p = screen.screenToDipPoint({ x: e.x, y: e.y });
  Object.assign(drag, { down: true, x0: p.x, y0: p.y });
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
    notify("Shiftwheel can't read the keyboard/mouse", err.message);
  }
}

function stopHook() {
  if (!hookRunning) return;
  try { uIOhook.stop(); } catch { /* already stopped */ }
  hookRunning = false;
  held.clear();
  drag.down = false;
  hideNow();
}

// ---------- Tray ----------

function buildMenu() {
  return Menu.buildFromTemplate([
    {
      label: 'Enabled',
      type: 'checkbox',
      checked: enabled,
      click: (item) => {
        enabled = item.checked;
        enabled ? startHook() : stopHook();
      },
    },
    {
      label: 'Sounds',
      type: 'checkbox',
      checked: soundOn,
      click: (item) => {
        soundOn = item.checked;
        saveSettings({ ...loadSettings(), sound: soundOn });
        if (overlay) overlay.webContents.send('overlay:settings', { sound: soundOn });
      },
    },
    {
      label: app.isPackaged ? 'Start with Windows' : 'Start with Windows (installed version only)',
      type: 'checkbox',
      checked: app.isPackaged && isAutostart(),
      enabled: app.isPackaged, // disabled in dev
      click: (item) => setAutostart(item.checked),
    },
    { type: 'separator' },
    { label: `Shiftwheel ${app.getVersion()}${app.isPackaged ? '' : ' (dev)'}`, enabled: false },
    { label: 'Quit', click: () => app.quit() },
  ]);
}

function trayImage() {
  // tray-16/24/32.png are drawn per size by scripts/make-ico.js (sharper than a resize)
  const img = nativeImage.createEmpty();
  for (const [scaleFactor, size] of [[1, 16], [1.5, 24], [2, 32]]) {
    img.addRepresentation({ scaleFactor, buffer: fs.readFileSync(path.join(__dirname, 'assets', `tray-${size}.png`)) });
  }
  return img;
}

function createTray() {
  tray = new Tray(trayImage());
  updateProgress();
  // The menu is rebuilt on every open so the autostart state always matches the registry
  const open = () => tray.popUpContextMenu(buildMenu());
  tray.on('click', open);
  tray.on('right-click', open);
}

// ---------- Background mode ----------

function startBackground() {
  convertOnly = false;
  clearTimeout(quitTimer);

  const settings = loadSettings();
  const firstRun = settings.firstRun !== false;
  soundOn = settings.sound !== false;
  if (firstRun) {
    setAutostart(true);
    saveSettings({ ...settings, firstRun: false });
  }

  createOverlay();
  createTray();
  startHook();

  powerMonitor.on('suspend', stopHook);
  powerMonitor.on('resume', () => { stopHook(); startHook(); }); // the hook can die after sleep

  if (firstRun && !hidden) notify('Shiftwheel is ready', 'Start dragging a file, then tap Shift to convert, Ctrl for tools, or Alt for actions.');
}

// ---------- Single instance ----------

if (!app.requestSingleInstanceLock({ argv: process.argv })) {
  app.quit();
} else {
  // Use additionalData: this event's own argv can be reordered on Windows
  app.on('second-instance', (_e, _argv, _cwd, data) => {
    const argv = (data && data.argv) || _argv;
    if (handleArgv(argv)) return;
    if (convertOnly) return startBackground(); // app opened normally while a --convert instance is still running
    if (!argv.includes('--hidden')) notify('Shiftwheel is already running', "It's in the tray. Start dragging a file, then tap Shift.");
  });

  app.whenReady().then(() => {
    if (convertOnly) handleArgv(process.argv);
    else startBackground();
  });

  app.on('will-quit', stopHook);
}
