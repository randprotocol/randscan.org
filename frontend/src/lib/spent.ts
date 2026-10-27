import type { Nullifier } from '@/types';

export interface NullifierPage {
  nullifiers: Nullifier[];
  next: string | null;
}

/**
 * Which of `owned` (a wallet's own nullifiers, derived from its viewing key) are published. The
 * whole public set is downloaded a page at a time and matched here, so no request ever carries a
 * nullifier of the visitor's: the only thing sent is the server's own cursor from the previous page.
 */
export async function matchSpent(
  owned: string[],
  fetchPage: (after: string | null) => Promise<NullifierPage>
): Promise<Map<string, Nullifier>> {
  const want = new Set(owned);
  const spent = new Map<string, Nullifier>();
  // The download runs even when nothing is owned, so every wallet makes the same requests.
  let after: string | null = null;
  for (;;) {
    const page: NullifierPage = await fetchPage(after);
    for (const nf of page.nullifiers) if (want.has(nf.nullifier)) spent.set(nf.nullifier, nf);
    if (!page.next || page.next === after) break;
    after = page.next;
  }
  return spent;
}
