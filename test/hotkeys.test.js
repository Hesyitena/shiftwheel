// Tests for hotkeys.js with synthetic key sequences: node test/hotkeys.test.js
const assert = require('assert');
const { createTaps } = require('../hotkeys');

const SHIFT = 42, SHIFT_R = 54, CTRL = 29, ALT = 56, A = 30;
const keys = { [SHIFT]: 'convert', [SHIFT_R]: 'convert', [CTRL]: 'tools', [ALT]: 'actions' };

let passed = 0, failed = 0;
function test(name, fn) {
  try { fn(); passed++; console.log(`OK    ${name}`); }
  catch (e) { failed++; console.log(`FAIL  ${name}: ${e.message}`); }
}

test('a normal tap opens; its auto-repeats are ignored', () => {
  const t = createTaps(keys);
  assert.strictEqual(t.down(SHIFT, 0), 'convert');
  for (let ms = 500; ms < 3000; ms += 33) assert.strictEqual(t.down(SHIFT, ms), null, `repeat at ${ms} ms`);
});

test('tapping again after a keyup works, even quickly', () => {
  const t = createTaps(keys);
  assert.strictEqual(t.down(SHIFT, 0), 'convert');
  t.up(SHIFT);
  assert.strictEqual(t.down(SHIFT, 120), 'convert');
});

test('a lost keyup no longer blocks the key forever (the reported bug)', () => {
  const t = createTaps(keys);
  assert.strictEqual(t.down(ALT, 0), 'actions');
  // keyup never arrives
  assert.strictEqual(t.down(ALT, 2000), 'actions', 'a tap 2 s later must count');
  assert.strictEqual(t.down(ALT, 9000), 'actions', 'and keep counting');
});

test('Shift pressed left but released right is treated as released', () => {
  const t = createTaps(keys);
  assert.strictEqual(t.down(SHIFT, 0), 'convert');
  t.up(SHIFT_R);
  assert.strictEqual(t.down(SHIFT, 100), 'convert');
});

test('switching keys works while another key is held', () => {
  const t = createTaps(keys);
  assert.strictEqual(t.down(SHIFT, 0), 'convert');
  assert.strictEqual(t.down(CTRL, 200), 'tools');
  assert.strictEqual(t.down(ALT, 400), 'actions');
  assert.strictEqual(t.down(SHIFT, 1500), 'convert'); // Shift's keyup was lost, but time has passed
});

test('unrelated keys are ignored', () => {
  const t = createTaps(keys);
  assert.strictEqual(t.down(A, 0), null);
  t.up(A);
});

test('reset forgets every key', () => {
  const t = createTaps(keys);
  t.down(CTRL, 0);
  t.reset();
  assert.strictEqual(t.down(CTRL, 10), 'tools');
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
