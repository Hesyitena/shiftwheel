// End-to-end tests for the UI pages (overlay, HUD, crop) in a real Chromium.
// The Tauri bridge is replaced by a recorder: backend events are emitted into the page and
// every invoke() the page makes is captured. The wheel hit-test is checked point by point
// against vectors exported by the Rust code (wheel.rs), generated here with `cargo test`.
//
//   npm run test:ui
//
// Needs Rust and Playwright with Chromium (`npm i -D playwright && npx playwright install chromium`).

import { createRequire } from 'node:module';
import { readFileSync, mkdtempSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');
const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const UI = join(ROOT, 'ui');

if (!process.env.SHIFTWHEEL_WHEEL_VECTORS) {
  const file = join(mkdtempSync(join(tmpdir(), 'shiftwheel-')), 'wheel.json');
  execFileSync('cargo', ['test', '--quiet', '--manifest-path', join(ROOT, 'src-tauri', 'Cargo.toml'), '--lib', 'export_hit_vectors'], {
    env: { ...process.env, SHIFTWHEEL_WHEEL_VECTORS: file },
    stdio: ['ignore', 'ignore', 'inherit'],
  });
  process.env.SHIFTWHEEL_WHEEL_VECTORS = file;
}

let failures = 0;
let passed = 0;
async function test(name, fn) {
  try {
    await fn();
    passed++;
    console.log(`ok   ${name}`);
  } catch (e) {
    failures++;
    console.log(`FAIL ${name}\n     ${e.stack || e}`);
  }
}
function assert(cond, msg) {
  if (!cond) throw new Error(msg || 'assertion failed');
}
function eq(a, b, msg) {
  const [x, y] = [JSON.stringify(a), JSON.stringify(b)];
  if (x !== y) throw new Error(`${msg || 'not equal'}: got ${x}, want ${y}`);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Installed before any page script: a fake Tauri global plus an oscillator counter.
const MOCK = ({ responses }) => {
  const listeners = {};
  window.__calls = [];
  window.__oscillators = [];
  window.__emit = (event, payload) => (listeners[event] || []).forEach((cb) => cb({ event, payload }));
  window.__TAURI__ = {
    event: { listen: async (event, cb) => { (listeners[event] ||= []).push(cb); return () => {}; } },
    core: {
      invoke: async (cmd, args) => {
        window.__calls.push({ cmd, args: args === undefined ? null : args });
        return responses[cmd] === undefined ? null : responses[cmd];
      },
    },
  };
  const Real = window.AudioContext;
  window.AudioContext = class extends Real {
    createOscillator() {
      const o = super.createOscillator();
      const start = o.start.bind(o);
      o.start = (...a) => { window.__oscillators.push(o.frequency.value); return start(...a); };
      return o;
    }
  };
};

async function open(browser, page, responses = {}) {
  const ctx = await browser.newContext({ viewport: { width: 980, height: 720 } });
  const p = await ctx.newPage();
  const errors = [];
  p.on('pageerror', (e) => errors.push(String(e)));
  p.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
  await p.addInitScript(MOCK, { responses: { get_settings: { sound: true }, ...responses } });
  await p.goto(pathToFileURL(join(UI, page)).href);
  await p.waitForTimeout(50);
  p.errors = errors;
  p.send = (event, payload) => p.evaluate(([e, d]) => window.__emit(e, d), [event, payload]);
  p.calls = (cmd) => p.evaluate((c) => window.__calls.filter((x) => !c || x.cmd === c), cmd);
  p.oscillators = () => p.evaluate(() => window.__oscillators.length);
  return p;
}

const item = (id, label, icon) => ({ id, label, caption: `${label} caption`, ...(icon ? { icon } : {}) });
const IMAGE_ITEMS = ['jpg', 'webp', 'avif', 'tiff', 'gif', 'bmp', 'pdf', 'txt'].map((f) => item(`convert:${f}`, f.toUpperCase()));
const TOOL_ITEMS = [item('tool:image.compress', 'Compress', 'compress'), item('tool:image.rotate', 'Rotate', 'rotate'), item('tool:image.gray', 'Black & white', 'gray')];
// Middle of slice i of n, like a user aiming at it
const aim = (i, n) => {
  const a = (-90 + (i * 360) / n) * Math.PI / 180;
  return { x: 180 + 113 * Math.cos(a), y: 180 + 113 * Math.sin(a) };
};

const browser = await chromium.launch();

// ------------------------------------------------------------------------------------------
// Overlay
// ------------------------------------------------------------------------------------------

await test('overlay: no mojibake in the mode tabs', async () => {
  const p = await open(browser, 'overlay.html');
  const text = await p.textContent('#modes');
  assert(text.includes('⇧Convert'), text);
  assert(!/[âÃ]/.test(await p.content()), 'mojibake left in page');
  eq(p.errors, [], 'page errors');
});

await test('overlay: settings are pulled on load (no startup race)', async () => {
  const p = await open(browser, 'overlay.html', { get_settings: { sound: false } });
  eq((await p.calls('get_settings')).length, 1);
  await p.send('overlay:show', { mode: 'convert', fresh: true });
  await p.send('overlay:drag_enter', { category: 'image', count: 1, items: IMAGE_ITEMS, x: 180, y: 180 });
  await p.send('overlay:sound', 'select');
  await sleep(50);
  eq(await p.oscillators(), 0, 'sound is off, nothing may play');
});

await test('overlay: a fresh wheel shows the dragged files at once', async () => {
  const p = await open(browser, 'overlay.html');
  await p.send('overlay:show', { mode: 'convert', fresh: true, items: IMAGE_ITEMS, paths: ['C:/a/photo.png', 'C:/a/b.png'], file_name: 'photo.png', category: 'image' });
  await sleep(50);
  assert(!(await p.$eval('#stage', (s) => s.classList.contains('probing'))), 'visible');
  eq(await p.$$eval('.seg', (s) => s.length), IMAGE_ITEMS.length, 'slices drawn');
  eq(await p.textContent('#fname'), 'photo.png +1');
  eq(await p.textContent('#title'), '2 images');
  eq((await p.calls('overlay_revealed')).length, 1);
  // the cursor position from the hook highlights a slice and points at it
  await p.send('overlay:drag_over', { x: 180, y: 60 });
  eq(await p.$eval('.seg.hover', (g) => g.dataset.id), IMAGE_ITEMS[0].id);
  assert(await p.$eval('#pointerGroup', (g) => g.classList.contains('active')), 'pointer shown');
});

await test('overlay: file drag reveals the backend list and highlights what will run', async () => {
  const p = await open(browser, 'overlay.html');
  await p.send('overlay:show', { mode: 'convert', fresh: true });
  await p.send('overlay:drag_enter', { category: 'image', count: 1, items: IMAGE_ITEMS, ...aim(2, IMAGE_ITEMS.length) });
  assert(!(await p.$eval('#stage', (s) => s.classList.contains('probing'))), 'revealed');
  eq(await p.$$eval('.seg', (s) => s.map((g) => g.dataset.id)), IMAGE_ITEMS.map((i) => i.id));
  eq(await p.evaluate(() => document.body.dataset.pick), 'convert:avif');
  eq(await p.textContent('#title'), 'AVIF');
  await p.send('overlay:drag_over', aim(6, IMAGE_ITEMS.length));
  eq(await p.evaluate(() => document.body.dataset.pick), 'convert:pdf');
  await p.send('overlay:drag_over', { x: 185, y: 175 });
  eq(await p.textContent('#title'), 'Cancel');
  eq(await p.evaluate(() => document.body.dataset.pick), '');
  eq((await p.calls('overlay_revealed')).map((c) => c.args), [{ ok: true }]);
  eq(p.errors, []);
});

await test('overlay: one sound per drop (backend decides)', async () => {
  const p = await open(browser, 'overlay.html');
  await p.send('overlay:show', { mode: 'convert', fresh: true });
  await p.send('overlay:drag_enter', { category: 'image', count: 1, items: IMAGE_ITEMS, x: 180, y: 180 });
  await sleep(30);
  const before = await p.oscillators();
  await p.send('overlay:sound', 'select');
  await p.send('overlay:drag_drop', { id: 'convert:jpg' });
  await sleep(30);
  // "select" is two notes with an octave harmonic each = 4 oscillators; a double sound is 8
  eq((await p.oscillators()) - before, 4);
});

await test('overlay: switching wheels mid-drag keeps the files', async () => {
  const p = await open(browser, 'overlay.html');
  await p.send('overlay:show', { mode: 'convert', fresh: true });
  await p.send('overlay:drag_enter', { category: 'image', count: 3, items: IMAGE_ITEMS, x: 5, y: 5 });
  eq(await p.textContent('#title'), '3 images');
  await p.send('overlay:show', { mode: 'tools', fresh: false, items: TOOL_ITEMS });
  eq(await p.$$eval('.seg', (s) => s.map((g) => g.dataset.id)), TOOL_ITEMS.map((i) => i.id));
  eq(await p.textContent('#title'), '3 images', 'category survives the switch');
  eq(await p.$eval('.mode.on', (m) => m.dataset.mode), 'tools');
  assert(!(await p.$eval('#stage', (s) => s.classList.contains('probing'))), 'still visible');
  await p.send('overlay:drag_over', aim(1, 3));
  eq(await p.evaluate(() => document.body.dataset.pick), 'tool:image.rotate');
});

await test('overlay: empty wheels explain themselves', async () => {
  const p = await open(browser, 'overlay.html');
  await p.send('overlay:show', { mode: 'convert', fresh: true });
  await p.send('overlay:drag_enter', { category: 'folder', count: 1, items: [], x: 180, y: 60 });
  eq(await p.textContent('#title'), 'Folders can’t be converted');
  await p.send('overlay:show', { mode: 'tools', fresh: false, items: [] });
  eq(await p.textContent('#title'), 'No tools for folders yet');
  await p.send('overlay:show', { mode: 'convert', fresh: true });
  await p.send('overlay:drag_enter', { category: 'unknown', count: 1, items: [], x: 180, y: 60 });
  eq(await p.textContent('#title'), 'Can’t convert this file type');
});

await test('overlay: hit-testing matches the backend for every point', async () => {
  const file = process.env.SHIFTWHEEL_WHEEL_VECTORS;
  assert(file, 'no hit-test vectors');
  const cases = JSON.parse(readFileSync(file, 'utf8'));
  const p = await open(browser, 'overlay.html');
  await p.send('overlay:show', { mode: 'convert', fresh: true });
  let mismatches = [];
  for (let count = 1; count <= 10; count++) {
    const items = Array.from({ length: count }, (_, i) => item(`convert:f${i}`, `F${i}`));
    await p.send('overlay:drag_enter', { category: 'image', count: 1, items, x: 0, y: 0 });
    const subset = cases.filter((c) => c.count === count);
    const got = await p.evaluate((pts) => pts.map(({ x, y }) => {
      const s = segmentAt(x, y);
      if (s === null) return null;
      if (s === hole) return 'hole';
      return [...segs.querySelectorAll('.seg')].indexOf(s);
    }), subset);
    subset.forEach((c, i) => { if (got[i] !== c.expect) mismatches.push({ ...c, got: got[i] }); });
  }
  eq(mismatches.slice(0, 5), [], `${mismatches.length} of ${cases.length} points disagree`);
  console.log(`     ${cases.length} points agree`);
});

// ------------------------------------------------------------------------------------------
// HUD
// ------------------------------------------------------------------------------------------

const hudMsg = (o) => ({ name: 'photo.png', label: 'Convert to JPG', chip: 'JPG', index: 1, total: 1, percent: null, state: 'busy', detail: '', clickable: false, ...o });

await test('hud: progress, then an honest failure', async () => {
  const p = await open(browser, 'hud.html');
  await p.send('hud:update', hudMsg({ index: 1, total: 2, percent: 0 }));
  eq(await p.textContent('#sub'), 'Convert to JPG  ·  1 of 2');
  eq(await p.textContent('#pct'), '0%');
  await p.send('hud:update', hudMsg({ state: 'error', label: 'Convert to JPG · Failed', detail: 'FFmpeg is not installed', percent: 1, index: 2, total: 2 }));
  eq(await p.textContent('#sub'), 'Convert to JPG · Failed');
  eq(await p.textContent('#detail'), 'FFmpeg is not installed');
  assert(await p.evaluate(() => document.body.classList.contains('error')), 'error style');
  assert(await p.isVisible('#detail'), 'reason visible');
  await p.click('#card');
  eq((await p.calls('hud_reveal')).length, 0, 'nothing to reveal');
  eq(p.errors, []);
});

await test('hud: finished card reveals the result on click', async () => {
  const p = await open(browser, 'hud.html');
  await p.send('hud:update', hudMsg({ state: 'done', label: 'Convert to JPG · Done', detail: 'Saved as photo.jpg', percent: 1, clickable: true }));
  eq(await p.textContent('#detail'), 'Saved as photo.jpg');
  eq(await p.textContent('#pct'), '');
  await p.click('#card');
  eq((await p.calls('hud_reveal')).length, 1);
  await p.send('hud:hide', null);
  assert(await p.evaluate(() => document.body.classList.contains('hidden')), 'hidden');
});

// ------------------------------------------------------------------------------------------
// Crop
// ------------------------------------------------------------------------------------------

const PNG_1x1 = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';
const cropReq = { src: PNG_1x1, width: 400, height: 200, name: 'a.png', count: 2 };

await test('crop: request sent before the page loaded is picked up', async () => {
  const p = await open(browser, 'crop.html', { get_crop_request: cropReq });
  eq(await p.textContent('#name'), 'a.png');
  eq(await p.textContent('#save'), 'Crop 2 images');
  eq(await p.textContent('#size'), '400 × 200 px');
  eq(p.errors, []);
});

await test('crop: ratio preset, save once', async () => {
  const p = await open(browser, 'crop.html');
  await p.send('crop:init', cropReq);
  await p.click('button[data-r="1"]');
  eq(await p.textContent('#size'), '200 × 200 px');
  await p.keyboard.press('Enter');
  await p.keyboard.press('Enter');
  await p.click('#save');
  const calls = await p.calls('crop_done');
  eq(calls.length, 1, 'answered exactly once');
  eq(calls[0].args.rect, { x: 0.25, y: 0, w: 0.5, h: 1 });
});

await test('crop: drawing a selection and Escape', async () => {
  const p = await open(browser, 'crop.html');
  await p.send('crop:init', cropReq);
  const b = await p.$eval('#frame', (f) => { const r = f.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height }; });
  await p.mouse.move(b.x + b.w * 0.1, b.y + b.h * 0.2);
  await p.mouse.down();
  await p.mouse.move(b.x + b.w * 0.6, b.y + b.h * 0.7, { steps: 5 });
  await p.mouse.up();
  eq(await p.textContent('#size'), '200 × 100 px');
  await p.keyboard.press('Escape');
  eq((await p.calls('crop_done')).map((c) => c.args.rect), [null]);
});

// ------------------------------------------------------------------------------------------
// Dot theme additions
// ------------------------------------------------------------------------------------------

await test('theme: bundled fonts load offline', async () => {
  const p = await open(browser, 'overlay.html');
  await p.send('overlay:show', { mode: 'convert', fresh: true });
  await p.send('overlay:drag_enter', { category: 'image', count: 1, items: IMAGE_ITEMS, ...aim(1, IMAGE_ITEMS.length) });
  const loaded = await p.evaluate(async () => {
    await document.fonts.ready;
    return ['900 34px Doto', '700 14px "Space Mono"', '400 12px "Space Mono"'].map((f) => document.fonts.check(f));
  });
  eq(loaded, [true, true, true]);
  eq(p.errors, []);
});

await test('overlay: missing helper program is shown on the slice and in the center', async () => {
  const p = await open(browser, 'overlay.html');
  const items = [
    { ...item('convert:jpg', 'JPG'), hint: 'small' },
    { ...item('convert:avif', 'AVIF'), hint: 'tiny', needs: 'ffmpeg' },
    { ...item('convert:pdf', 'PDF'), hint: 'doc' },
  ];
  await p.send('overlay:show', { mode: 'convert', fresh: true });
  await p.send('overlay:drag_enter', { category: 'image', count: 3, file_name: 'holiday.png', items, x: 5, y: 5 });
  eq(await p.textContent('#fname'), 'holiday.png +2');
  eq(await p.$$eval('.seg', (s) => s.map((g) => g.classList.contains('missing'))), [false, true, false]);
  await p.send('overlay:drag_over', aim(1, 3));
  eq(await p.textContent('#title'), 'AVIF');
  eq(await p.textContent('#desc'), 'Needs FFmpeg');
  assert(await p.$eval('#center', (c) => c.classList.contains('missing')), 'center marks it');
  // the drop still resolves to that slice (the job then explains what to install)
  eq(await p.evaluate(() => document.body.dataset.pick), 'convert:avif');
});

await test('first run: shown once, choice is sent back', async () => {
  const p = await open(browser, 'hud.html', { get_first_run: { autostart: false } });
  assert(await p.evaluate(() => document.body.classList.contains('first')), 'first-run view');
  assert(await p.isVisible('#first'), 'card visible');
  eq(await p.isChecked('#autostart'), false);
  await p.check('#autostart');
  await p.click('#gotit');
  eq((await p.calls('first_run_done')).map((c) => c.args), [{ autostart: true }]);
  assert(!(await p.isVisible('#first')), 'card gone');
  eq(p.errors, []);
});

await test('first run: a job replaces it with the progress card', async () => {
  const p = await open(browser, 'hud.html');
  await p.send('hud:first_run', { autostart: true });
  eq(await p.isChecked('#autostart'), true);
  await p.send('hud:update', hudMsg({ index: 1, total: 3, percent: 0 }));
  assert(!(await p.isVisible('#first')), 'first run hidden');
  eq(await p.textContent('#count'), '1/3');
  eq(await p.$$eval('#bar i.head', (d) => d.length), 1);
});

await test('crop: title-bar close cancels', async () => {
  const p = await open(browser, 'crop.html');
  await p.send('crop:init', cropReq);
  await p.click('#close');
  eq((await p.calls('crop_done')).map((c) => c.args.rect), [null]);
});

await browser.close();
console.log(`\n${passed} passed, ${failures} failed`);
process.exit(failures ? 1 : 0);
