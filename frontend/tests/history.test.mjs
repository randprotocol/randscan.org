// Audit v14, SCAN-841 / VK-751: the history page's balance sums only notes the pasted key owns,
// and a row is Sent only when the key's own pk created the note.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const supported = process.features.typescript;
const skip = supported ? false : 'needs Node with TypeScript type stripping';
const { admit, isOwned, spendableNotes } = supported ? await import('../src/lib/history.ts') : {};

const ME = 'a'.repeat(64);
const OTHER = 'b'.repeat(64);
const note = (role, pk, from, amount = '5') => ({ role, pk, from, amount, asset: 0 });
const row = (n, state = 'unspent') => ({ note: n, spent: { state } });

test('a received row owned by another pk never reaches the balance', { skip }, () => {
  const phantom = note('received', OTHER, OTHER, '100000000000000');
  const mine = note('received', ME, OTHER, '7');
  assert.equal(isOwned(phantom, ME), false);
  assert.equal(isOwned(mine, ME.toUpperCase()), true);
  assert.equal(admit(phantom, ME), null, 'the phantom row is dropped from the history');
  assert.deepEqual(spendableNotes([row(phantom), row(mine)], ME), [mine]);
  // No pk for the key: nothing is owned.
  assert.deepEqual(spendableNotes([row(mine)], null), []);
});

test('only unspent received rows are spendable', { skip }, () => {
  const a = note('received', ME, OTHER, '1');
  const b = note('received', ME, OTHER, '2');
  const c = note('received', ME, OTHER, '4');
  const sent = note('sent', OTHER, ME, '8');
  const unverified = note('sent_unverified', OTHER, '0'.repeat(64), '16');
  const got = spendableNotes([row(a), row(b, 'spent'), row(c, 'unknown'), row(sent), row(unverified)], ME);
  assert.deepEqual(got, [a]);
});

test('a sent row is Sent only when the key created the note', { skip }, () => {
  const mine = note('sent', OTHER, ME);
  const planted = note('sent', OTHER, OTHER);
  assert.equal(admit(mine, ME), mine);
  assert.equal(admit(planted, ME).role, 'sent_unverified');
  const unverified = note('sent_unverified', OTHER, '0'.repeat(64));
  assert.equal(admit(unverified, ME), unverified);
  const byTxKey = note('tx_key', OTHER, OTHER);
  assert.equal(admit(byTxKey, ME), byTxKey);
});

test('the history page sums its balance through the ownership check', () => {
  const page = readFileSync(fileURLToPath(new URL('../src/app/[lang]/viewing/page.tsx', import.meta.url)), 'utf8');
  assert.match(page, /tokenBalances\(\s*spendableNotes\(rows \?\? \[\], info\?\.pk\)/, 'the balance is not summed from spendableNotes');
  assert.match(page, /admit\(/, 'opened rows are not passed through admit');
  assert.ok(!/role === 'received'/.test(page), 'the page decides ownership from the role string alone');
});
