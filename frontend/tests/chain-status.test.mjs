// The site-sync classifier (src/lib/chainStatus.ts), the build's chain pin (src/lib/chainPin.ts)
// and the <meta> tags that carry it into every page's HTML (src/components/ChainMeta.ts), which
// the monorepo's site-sync cron greps for. TypeScript loaded the way utils.test.mjs does it:
// Node's type stripping plus a resolve hook for `@/`. On a Node without either the file skips.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { existsSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';

const src = fileURLToPath(new URL('../src/', import.meta.url));
const supported = typeof registerHooks === 'function' && process.features.typescript;
const skip = supported ? false : 'needs Node with module.registerHooks and TypeScript type stripping';

let cs, pin, meta, React, server;
if (supported) {
  registerHooks({
    resolve(specifier, context, nextResolve) {
      if (specifier.startsWith('@/') || (specifier.startsWith('./') && context.parentURL?.includes('/src/'))) {
        const base = specifier.startsWith('@/')
          ? src + specifier.slice(2)
          : fileURLToPath(new URL(specifier, context.parentURL));
        for (const ext of ['.ts', '.js', '/index.ts']) {
          if (!/\.\w+$/.test(base) && existsSync(base + ext)) {
            return nextResolve(pathToFileURL(base + ext).href, context);
          }
        }
      }
      return nextResolve(specifier, context);
    },
  });
  cs = await import('../src/lib/chainStatus.ts');
  pin = await import('../src/lib/chainPin.ts');
  meta = await import('../src/components/ChainMeta.ts');
  React = await import('react');
  server = await import('react-dom/server');
}

const G = 'ca76c2766ed6090ac6ba5dad86b724dd5d2b80c0edfaed287cd8b129e663ca9a';
const NOW = 1_800_000_000_000;
const view = (over = {}) => ({
  chain_id: 21,
  genesis_hash: G,
  head_height: 495_700,
  head_timestamp_ms: NOW - 2_500,
  node_height: 495_700,
  lag: 0,
  node_chain_id: 21,
  node_genesis_hash: G,
  halted: false,
  node_connected: true,
  ...over,
});
const state = (v) => cs.classify(v, pin.EXPECTED_CHAIN, NOW).state;
const reason = (v) => cs.classify(v, pin.EXPECTED_CHAIN, NOW).reason;

test('the pin is chain 21 and the thresholds are the spec\'s', { skip }, () => {
  assert.deepEqual(pin.EXPECTED_CHAIN, { chain_id: 21, genesis_hash: G });
  assert.equal(cs.STALL_MS, 60_000);
  assert.equal(cs.MAX_LAG, 10);
  assert.equal(cs.POLL_MS, 15_000);
  assert.equal(cs.FETCH_TIMEOUT_MS, 8_000);
});

test('a fetch error, nothing, or an unparsable reply is unreachable', { skip }, () => {
  assert.equal(state(new Error('timeout')), 'unreachable');
  assert.equal(state(null), 'unreachable');
  assert.equal(state(undefined), 'unreachable');
  assert.equal(state({ nonsense: true }), 'unreachable');
  assert.equal(reason(null), 'fetch');
});

test('a fresh head on the pinned chain is live', { skip }, () => {
  assert.equal(state(view()), 'live');
  assert.equal(cs.classify(view(), pin.EXPECTED_CHAIN, NOW).detail, 'Live at #495700.');
});

test('the stall boundary: 60 000 ms is fresh, 60 001 ms is stalled', { skip }, () => {
  assert.equal(state(view({ head_timestamp_ms: NOW - 60_000 })), 'live');
  assert.equal(state(view({ head_timestamp_ms: NOW - 60_001 })), 'stalled');
  assert.equal(reason(view({ head_timestamp_ms: NOW - 60_001 })), 'stale');
});

test('a missing head timestamp is stalled', { skip }, () => {
  assert.equal(state(view({ head_timestamp_ms: null })), 'stalled');
  assert.equal(reason(view({ head_timestamp_ms: null })), 'noBlock');
});

test('the lag boundary: 10 is live, 11 is behind', { skip }, () => {
  assert.equal(state(view({ lag: 10 })), 'live');
  assert.equal(state(view({ lag: 11 })), 'behind');
  assert.equal(state(view({ lag: null })), 'live', 'no indexer, no lag');
});

test('a missing or different genesis is the wrong chain', { skip }, () => {
  assert.equal(state(view({ genesis_hash: null })), 'wrong_chain');
  assert.equal(reason(view({ genesis_hash: null })), 'genesis');
  assert.equal(state(view({ genesis_hash: 'ab'.repeat(32) })), 'wrong_chain');
});

test('a chain-id mismatch with a matching genesis is the wrong chain', { skip }, () => {
  assert.equal(state(view({ chain_id: 20 })), 'wrong_chain');
  assert.equal(reason(view({ chain_id: 20 })), 'chainId');
});

test('a node that moved to another chain under the indexer is the wrong chain', { skip }, () => {
  const v = view({ node_genesis_hash: 'cd'.repeat(32), halted: true, node_height: null, lag: null });
  assert.equal(state(v), 'wrong_chain');
  assert.equal(reason(v), 'halted');
  assert.equal(state(view({ node_chain_id: 22 })), 'wrong_chain');
});

test('an explorer cut off from its node is unreachable until the head goes stale', { skip }, () => {
  assert.equal(state(view({ node_connected: false })), 'unreachable');
  assert.equal(reason(view({ node_connected: false })), 'node');
  assert.equal(state(view({ node_connected: false, head_timestamp_ms: NOW - 60_001 })), 'stalled');
});

test('hashes compare without case or a 0x prefix', { skip }, () => {
  assert.equal(state(view({ genesis_hash: `0x${G.toUpperCase()}` })), 'live');
});

test('the order: wrong chain beats stalled beats behind', { skip }, () => {
  assert.equal(state(view({ lag: 50, head_timestamp_ms: NOW - 600_000 })), 'stalled');
  assert.equal(state(view({ chain_id: 20, lag: 50, head_timestamp_ms: NOW - 600_000 })), 'wrong_chain');
});

test('pill tone and banner per state', { skip }, () => {
  assert.equal(cs.tone('live'), 'positive');
  assert.equal(cs.tone('behind'), 'warning');
  for (const s of ['stalled', 'wrong_chain', 'unreachable']) assert.equal(cs.tone(s), 'negative');
  assert.equal(cs.tone(null), 'mute');
  assert.equal(cs.showsBanner('live'), false);
  assert.equal(cs.showsBanner(null), false, 'no banner before the first answer');
  for (const s of ['behind', 'stalled', 'wrong_chain', 'unreachable']) assert.equal(cs.showsBanner(s), true);
});

test('every page\'s <head> carries the pin as rand-genesis / rand-chain-id meta tags', { skip }, () => {
  const html = server.renderToStaticMarkup(React.createElement(meta.ChainMeta));
  assert.equal(html, `<meta name="rand-genesis" content="${G}"/><meta name="rand-chain-id" content="21"/>`);
  const other = server.renderToStaticMarkup(
    React.createElement(meta.ChainMeta, { pin: { chain_id: 22, genesis_hash: 'ef'.repeat(32) } }),
  );
  assert.match(other, /content="22"/);
});

test('the banner names each state in a sentence every language has', { skip }, async () => {
  const { readFileSync, readdirSync } = await import('node:fs');
  const dir = new URL('../src/i18n/messages/', import.meta.url);
  const dicts = readdirSync(dir).filter((f) => f.endsWith('.json')).map((f) => [f, JSON.parse(readFileSync(new URL(f, dir), 'utf8'))]);
  const at = (d, key) => key.split('.').reduce((o, k) => o?.[k], d);
  const cases = [
    [view({ lag: 11 }), 'behind', 'lag'],
    [view({ head_timestamp_ms: NOW - 240_000 }), 'stalled', 'stale'],
    [view({ head_timestamp_ms: null }), 'stalled', 'noBlock'],
    [view({ chain_id: 20 }), 'wrong_chain', 'chainId'],
    [view({ genesis_hash: 'ab'.repeat(32) }), 'wrong_chain', 'genesis'],
    [view({ node_genesis_hash: 'cd'.repeat(32) }), 'wrong_chain', 'halted'],
    [view({ node_connected: false }), 'unreachable', 'node'],
    [null, 'unreachable', 'fetch'],
  ];
  for (const [v, st, r] of cases) {
    const c = cs.classify(v, pin.EXPECTED_CHAIN, NOW);
    assert.deepEqual([c.state, c.reason], [st, r]);
    const m = cs.bannerMessage(c, v, pin.EXPECTED_CHAIN);
    assert.equal(m.key, `chainStatus.banner.${r}`);
    for (const [f, d] of dicts) assert.equal(typeof at(d, m.key), 'string', `${f} lacks ${m.key}`);
    for (const s of ['live', 'behind', 'stalled', 'wrong_chain', 'unreachable', 'checking'])
      for (const [f, d] of dicts) assert.equal(typeof at(d, `chainStatus.states.${s}`), 'string', `${f} lacks ${s}`);
    // The last good height comes with every banner that has one; a chain with no block has none.
    assert.equal(m.lastGood === null, v === null || r === 'noBlock', r);
  }
  const wrong = cs.bannerMessage(cs.classify(view({ chain_id: 20 }), pin.EXPECTED_CHAIN, NOW), view({ chain_id: 20 }), pin.EXPECTED_CHAIN);
  assert.deepEqual([wrong.vars.shown, wrong.vars.expected], [20, 21]);
  assert.equal(cs.bannerMessage(cs.classify(view(), pin.EXPECTED_CHAIN, NOW), view(), pin.EXPECTED_CHAIN), null, 'live: no banner');
});
