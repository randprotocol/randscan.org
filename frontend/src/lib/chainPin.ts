// The chain this build of the explorer expects to be showing (site-sync design §1). A build-time
// constant, the one place the frontend pins it: the status pill and banner classify against it,
// and the root layout prints it as <meta name="rand-genesis"> / <meta name="rand-chain-id"> so the
// monorepo's site-sync cron can grep the deployed HTML for the live genesis. Move it at every
// chain cut, together with the API's RANDSCAN_EXPECTED_* (whose default is the same chain).

export interface ChainPin {
  chain_id: number;
  genesis_hash: string;
}

/** Chain 21, cut 2026-10-12. */
export const EXPECTED_CHAIN: ChainPin = {
  chain_id: 21,
  genesis_hash: 'ca76c2766ed6090ac6ba5dad86b724dd5d2b80c0edfaed287cd8b129e663ca9a',
};

/** The <meta> tags that carry the pin in every page's <head>. */
export function chainMetaTags(pin: ChainPin = EXPECTED_CHAIN): { name: string; content: string }[] {
  return [
    { name: 'rand-genesis', content: pin.genesis_hash },
    { name: 'rand-chain-id', content: String(pin.chain_id) },
  ];
}
