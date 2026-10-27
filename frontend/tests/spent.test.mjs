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

test('matchSpent downloads the whole set even when nothing is owned', { skip }, async () => {
  const sent = [];
  const pages = { '': { nullifiers: [nf('1')], next: 'c1' }, c1: { nullifiers: [nf('2')], next: null } };
  const spent = await matchSpent([], async (after) => { sent.push(after); return pages[after ?? '']; });
  assert.equal(spent.size, 0);
  assert.deepEqual(sent, [null, 'c1']);
});

test('every link rendered from a viewing key result has prefetch disabled', () => {
  const files = [
    'app/[lang]/viewing/page.tsx',
    'components/OpenedNotes.tsx',
    'components/TransactionOpener.tsx',
  ];
  for (const f of files) {
    const text = readFileSync(join(src, f), 'utf8');
    // every <L ...> / <Link ...> opening tag, and every <Hash ... href=...> tag
    const tags = [...text.matchAll(/<(L|Link|Hash)\b[^>]*?(?:=>[^>]*?)*>/gs)].map((m) => m[0]);
    for (const tag of tags) {
      const isLink = /^<(L|Link)\b/.test(tag);
      const hasHref = /\bhref=/.test(tag);
      if ((isLink || hasHref) && (isLink || /^<Hash/.test(tag))) {
        if (isLink || hasHref) assert.ok(/prefetch=\{false\}/.test(tag), `${f}: a link without prefetch={false}: ${tag.slice(0, 90)}`);
      }
    }
    assert.ok(tags.length > 0 || f.endsWith('TransactionOpener.tsx'), `${f}: no links found, the check is vacuous`);
  }
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
