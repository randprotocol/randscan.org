// The history scan decides which notes are spent by matching the public nullifier set in the
// browser. No request may carry a nullifier of the visitor's.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const src = fileURLToPath(new URL('../src/', import.meta.url));
const supported = process.features.typescript;
const skip = supported ? false : 'needs Node with TypeScript type stripping';
const { matchSpent } = supported ? await import('../src/lib/spent.ts') : {};

const nf = (c) => ({ nullifier: c.repeat(64), tx_hash: 'f'.repeat(64), height: 1, tx_index: 0 });

test('matchSpent finds the owned nullifiers across pages and sends only server cursors', { skip }, async () => {
  const mine = ['1', '4'].map((c) => c.repeat(64));
  const pages = {
    '': { nullifiers: [nf('1'), nf('2')], next: 'c2' },
    c2: { nullifiers: [nf('3'), nf('4')], next: 'c4' },
    c4: { nullifiers: [nf('5')], next: null },
  };
  const sent = [];
  const spent = await matchSpent([...mine, '9'.repeat(64)], async (after) => {
    sent.push(after);
    return pages[after ?? ''];
  });
  assert.deepEqual([...spent.keys()].sort(), mine);
  assert.deepEqual(sent, [null, 'c2', 'c4']);
  for (const a of sent) assert.ok(a === null || !mine.includes(a) && /^c\d$/.test(a), 'a request carried a wallet value');
});

test('matchSpent with nothing owned makes no request', { skip }, async () => {
  let calls = 0;
  const spent = await matchSpent([], async () => { calls += 1; return { nullifiers: [], next: null }; });
  assert.equal(spent.size, 0);
  assert.equal(calls, 0);
});

test('the frontend has no route that takes a nullifier in a body or in a lookup', () => {
  const walk = (d) => readdirSync(d).flatMap((f) => (statSync(join(d, f)).isDirectory() ? walk(join(d, f)) : [join(d, f)]));
  for (const f of walk(src).filter((f) => /\.(ts|tsx)$/.test(f))) {
    const text = readFileSync(f, 'utf8');
    assert.ok(!/nullifiers\/lookup|lookupNullifiers/.test(text), `${f} posts nullifiers`);
  }
  const opener = readFileSync(join(src, 'components/TransactionOpener.tsx'), 'utf8');
  assert.ok(!/getNullifier\(/.test(opener), 'TransactionOpener asks the server about a note nullifier');
});
