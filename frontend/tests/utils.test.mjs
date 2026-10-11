// The pure helpers of src/lib/utils.ts: search-query classification, hash and address
// shortening, transaction-kind labels, token balances, bridge formatting. utils.ts is TypeScript
// with `@/` imports, so the test loads it through Node's type stripping and a resolve hook that
// maps `@/x` to src/x (no bundler, no new package). On a Node without either feature the file
// skips rather than fails.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { existsSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';

const src = fileURLToPath(new URL('../src/', import.meta.url));
const supported = typeof registerHooks === 'function' && process.features.typescript;
const skip = supported ? false : 'needs Node with module.registerHooks and TypeScript type stripping';

let u;
if (supported) {
  registerHooks({
    resolve(specifier, context, nextResolve) {
      if (specifier.startsWith('@/')) {
        const base = src + specifier.slice(2);
        for (const ext of ['', '.ts', '.js', '/index.ts', '/index.js']) {
          if (existsSync(base + ext) && !base.endsWith('/') && (ext !== '' || /\.\w+$/.test(base))) {
            return nextResolve(pathToFileURL(base + ext).href, context);
          }
        }
      }
      return nextResolve(specifier, context);
    },
  });
  u = await import('../src/lib/utils.ts');
}

const HASH = 'ab'.repeat(32);
const ADDR = '2nRdFChBXRmKoe2sQE3ZYDzvdg53QmBZJJ9iweY7hk1v';

test('cn joins only the truthy class names', { skip }, () => {
  assert.equal(u.cn('a', undefined, 'b', false, null, ''), 'a b');
  assert.equal(u.cn(), '');
});

test('shortenHash keeps both ends and leaves short values whole', { skip }, () => {
  assert.equal(u.shortenHash(HASH), 'abababab…ababab');
  assert.equal(u.shortenHash(HASH, 4, 4), 'abab…abab');
  assert.equal(u.shortenHash('short'), 'short');
  assert.equal(u.shortenHash('x'.repeat(17)), 'x'.repeat(17), 'at the boundary nothing is elided');
  assert.equal(u.shortenHash('x'.repeat(18)), 'xxxxxxxx…xxxxxx');
  assert.equal(u.shortenHash(null), '—');
  assert.equal(u.shortenHash(undefined), '—');
  assert.equal(u.shortenHash(''), '—');
});

test('a search term is a height, a hash, an address or nothing', { skip }, () => {
  assert.equal(u.getSearchQueryType('  1234 '), 'height');
  assert.equal(u.getSearchQueryType('0'), 'height');
  assert.equal(u.getSearchQueryType(HASH), 'hash');
  assert.equal(u.getSearchQueryType('0x' + HASH), 'hash');
  assert.equal(u.getSearchQueryType(HASH.toUpperCase()), 'hash');
  assert.equal(u.getSearchQueryType(ADDR), 'address');
  assert.equal(u.getSearchQueryType(''), 'unknown');
  assert.equal(u.getSearchQueryType('   '), 'unknown');
  assert.equal(u.getSearchQueryType('hello world'), 'unknown');
  assert.equal(u.getSearchQueryType('12ab'), 'unknown');
  // 63 hex digits is neither a hash nor (it has a 0) base58.
  assert.equal(u.getSearchQueryType('a'.repeat(63)), 'unknown');
  assert.equal(u.isAddress(HASH), false, 'a 64-hex string is a hash even though it is base58-shaped');
  assert.equal(u.isAddress('0OIl' + 'a'.repeat(30)), false, 'the base58 alphabet has no 0, O, I or l');
  assert.equal(u.isHeight('-5'), false);
  assert.equal(u.isHash(HASH.slice(2)), false);
});

test('bridge token totals count a token once however many backings repeat it', { skip }, () => {
  const rows = [
    { index: 1, token_deposits: 5, token_burns: 2 },
    { index: 1, token_deposits: 5, token_burns: 2 },
    { index: 2, token_deposits: 1, token_burns: 0 },
  ];
  assert.deepEqual(u.bridgeTokenTotals(rows), { deposits: 6, burns: 2 });
  assert.deepEqual(u.bridgeTokenTotals([]), { deposits: 0, burns: 0 });
});

test('every filterable kind has a label, a short label and a badge', { skip }, () => {
  assert.equal(new Set(u.TRANSACTION_KINDS).size, u.TRANSACTION_KINDS.length);
  assert.ok(!u.TRANSACTION_KINDS.includes('other'), '"other" is display-only');
  for (const k of [...u.TRANSACTION_KINDS, 'other']) {
    assert.notEqual(u.getKindLabel(k), k, `${k} has a label`);
    assert.notEqual(u.getKindShortLabel(k), k, `${k} has a short label`);
    assert.match(u.getKindBadgeClass(k), /^badge badge-\w+$/);
  }
  assert.equal(u.getKindLabel('transfer'), 'Transfer (shielded)');
  assert.equal(u.getKindShortLabel('bridge_burn'), 'Bridge out');
  assert.equal(u.getKindBadgeClass('bond'), 'badge badge-stake');
  // A kind from a newer node is shown as it came.
  assert.equal(u.getKindLabel('future_kind'), 'future_kind');
  assert.equal(u.getKindShortLabel('future_kind'), 'future_kind');
  assert.equal(u.getKindBadgeClass('future_kind'), 'badge badge-neutral');
});

test('only bridge attestations and burns carry a bridged unit', { skip }, () => {
  assert.equal(u.amountIsBridged('bridge_attest'), true);
  assert.equal(u.amountIsBridged('bridge_burn'), true);
  assert.equal(u.amountIsBridged('token_mint'), false);
  assert.equal(u.amountIsBridged('mint'), false);
});

const ZUSD = { index: 3, symbol: 'zUSD', decimals: 6, authority: { kind: 'bridge' } };
const FAKE_ZUSD = { index: 1, symbol: 'zUSD', decimals: 6, authority: { kind: 'owner' } };
const OTHER = { index: 7, symbol: 'ABC', decimals: 2, authority: { kind: 'owner' } };
const TOKENS = [OTHER, FAKE_ZUSD, ZUSD];

test('a token is resolved by registry index, or not at all', { skip }, () => {
  assert.equal(u.resolveToken(TOKENS, 7), OTHER);
  assert.equal(u.resolveToken(TOKENS, 99), undefined);
  assert.equal(u.resolveToken(TOKENS, null), undefined);
  assert.equal(u.resolveToken(TOKENS, undefined), undefined);
  assert.equal(u.resolveToken(null, 7), undefined);
  assert.equal(u.resolveToken(undefined, 7), undefined);
});

test('token amounts use the registered decimals and symbol, never RAND\'s', { skip }, () => {
  assert.equal(u.formatTokenAmount('12500000', 3, TOKENS), '12.5 zUSD');
  assert.equal(u.formatTokenAmount('1250', 7, TOKENS), '12.5 ABC');
  assert.equal(u.formatTokenAmount('1500000000', 0, TOKENS), '1.5 RAND');
  assert.equal(u.formatTokenAmount('1500000000', null, TOKENS), '1.5 RAND');
  assert.equal(u.formatTokenAmount(null, 3, TOKENS), '—');
  assert.equal(u.formatTokenAmount(undefined, 3, TOKENS), '—');
  const unknown = u.formatTokenAmount('42', 99, TOKENS);
  assert.ok(unknown.includes('42') && unknown.includes('99'), unknown);
  assert.ok(!unknown.includes('RAND'));
});

test('the default tokens are the bridge-backed zUSD, found by authority and not by symbol or index', { skip }, () => {
  assert.deepEqual(u.defaultTokens(TOKENS), [ZUSD]);
  assert.deepEqual(u.defaultTokens([FAKE_ZUSD, OTHER]), []);
  assert.deepEqual(u.defaultTokens([]), []);
  assert.deepEqual(u.defaultTokens(null), []);
  const early = { ...ZUSD, index: 2 };
  assert.equal(u.defaultTokens([ZUSD, early])[0], early, 'the lowest index of several');
});

test('a balance list is RAND, then zUSD, then the rest by index', { skip }, () => {
  const rows = u.tokenBalances(
    [
      { asset: 7, amount: '100' },
      { asset: 0, amount: '5' },
      { asset: 0, amount: '6' },
      { asset: 5, amount: '1' },
    ],
    TOKENS,
  );
  assert.deepEqual(
    rows.map((r) => [r.asset, r.units, r.notes]),
    [
      [0, 11n, 2],
      [3, 0n, 0],
      [5, 1n, 1],
      [7, 100n, 1],
    ],
    'zUSD is listed at zero when none is held',
  );
  assert.equal(rows[1].token, ZUSD);
  assert.equal(rows[2].token, undefined, 'an unregistered asset has no token row');
  const empty = u.tokenBalances([], null);
  assert.deepEqual(empty.map((r) => [r.asset, r.units, r.notes]), [[0, 0n, 0]]);
});

test('bridge chains have names and a fallback', { skip }, () => {
  assert.equal(u.knownBridgeChainName(2), 'Ethereum');
  assert.equal(u.knownBridgeChainName(9), null);
  assert.equal(u.knownBridgeChainName(null), null);
  assert.equal(u.bridgeChainName(5), 'Solana');
  assert.equal(u.bridgeChainName(9), 'chain 9');
  assert.equal(u.bridgeChainName(undefined), '—');
  assert.deepEqual(u.BRIDGE_SOURCE_CHAINS.map((c) => c.name), ['Ethereum', 'BSC', 'Solana', 'Tron']);
  assert.match(u.formatBridgeChain(2), /Ethereum/);
});

test('a padded EVM address is shown as its 20 bytes', { skip }, () => {
  const evm = 'f10befe1e0794722d3baf8bfd5bdac47b2a33148';
  assert.equal(u.formatBridgeAddress('0'.repeat(24) + evm), '0x' + evm);
  assert.equal(u.formatBridgeAddress('0x' + '0'.repeat(24) + evm.toUpperCase()), '0x' + evm);
  const solana = 'd3e58f1e9317bbc3c69b63fadff558ea82ba5d00765f1f1e483d705d209b413a';
  assert.equal(u.formatBridgeAddress(solana), solana, 'a full 32-byte address is left whole');
  assert.equal(u.formatBridgeAddress('0xABCD'), 'abcd');
});

test('node roles and endpoints', { skip }, () => {
  assert.equal(u.formatEndpoint('1.2.3.4', 9000), '1.2.3.4:9000');
  assert.equal(u.formatEndpoint('1.2.3.4', null), '1.2.3.4');
  assert.equal(u.formatEndpoint('1.2.3.4', 0), '1.2.3.4:0');
  assert.equal(u.formatEndpoint(null, 9000), '—');
  assert.equal(u.formatEndpoint('', 9000), '—');
  assert.equal(u.getNodeRoleLabel('validator'), 'Validator');
  assert.equal(u.getNodeRoleLabel(''), '');
  assert.equal(u.getNodeRoleBadgeClass('validator'), 'badge badge-transfer');
  assert.equal(u.getNodeRoleBadgeClass('observer'), 'badge badge-mint');
  assert.equal(u.getNodeRoleBadgeClass('mystery'), 'badge badge-neutral');
});

test('the English wrappers print what the explorer always printed', { skip }, () => {
  assert.equal(u.formatUnits('1500000000'), '1.5');
  assert.equal(u.formatAmount('1000000000'), '1 RAND');
  assert.equal(u.formatStake('1000000000000', 'total stake'), '1,000 RAND total stake');
  assert.equal(u.formatNumber(1234567), '1,234,567');
  assert.equal(u.formatCompactNumber(2500000), '2.5M');
  assert.equal(u.formatBytes(null).length > 0, true);
  assert.equal(u.formatBinaryBytes(8388608), '8 MiB');
  assert.equal(u.formatBinaryBytes(65536), '64 KiB');
  assert.equal(u.formatMintWindow(86400), '24 hours');
  assert.equal(u.formatMintWindow(90 * 60), '90 minutes');
  assert.equal(u.formatMintWindow(45), '45 seconds');
  assert.equal(u.TOKEN_SYMBOL, 'RAND');
  assert.equal(u.TOKEN_DECIMALS, 9);
});

test('clipboard copy reports failure instead of throwing', { skip }, async () => {
  // Node has no navigator.clipboard: the helper must answer false.
  assert.equal(await u.copyToClipboard('text'), false);
  const calls = [];
  Object.defineProperty(globalThis, 'navigator', {
    configurable: true,
    value: { clipboard: { writeText: async (t) => calls.push(t) } },
  });
  assert.equal(await u.copyToClipboard('hello'), true);
  assert.deepEqual(calls, ['hello']);
});
