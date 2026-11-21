// The chain this build of the explorer expects to be showing (site-sync design §1). A build-time
// constant, the one place the frontend pins it: the status pill classifies against it,
// and the root layout prints it as <meta name="rand-genesis"> / <meta name="rand-chain-id"> so the
// monorepo's site-sync cron can grep the deployed HTML for the live genesis. Move it at every
// chain cut, together with the API's RANDSCAN_EXPECTED_* (whose default is the same chain).

export interface ChainPin {
  chain_id: number;
  genesis_hash: string;
}

/** Chain 21, cut 2026-10-12. */
export const EXPECTED_CHAIN: ChainPin = {
  chain_id: 22,
  genesis_hash: '29d67542941f2eb6f2c1bb68c759926f5d814c9e2c7a934292806a2d98b7e9c0',
};

/** The <meta> tags that carry the pin in every page's <head>. */
export function chainMetaTags(pin: ChainPin = EXPECTED_CHAIN): { name: string; content: string }[] {
  return [
    { name: 'rand-genesis', content: pin.genesis_hash },
    { name: 'rand-chain-id', content: String(pin.chain_id) },
  ];
}
