/**
 * Which opened rows are a viewing key's own (audit v14, SCAN-841 and VK-751).
 *
 * An envelope that opens is not ownership and not authorship: anyone can seal a note it owns
 * itself to a published address, and anyone who holds a viewing key can wrap a payment of its
 * own under that key's ovk. The WebAssembly opener (crates/randscan-viewing, `classify`) already
 * compares the note's owner and creator with the key's own pk; this restates the two comparisons
 * on the page's side, so the balance never rests on the role string alone.
 */

/** The fields of an opened note these checks read (`OpenedNote` of `@/lib/viewing`). */
export interface RoleFields {
  role: string;
  /** The note's owner. */
  pk: string;
  /** The note's creator. */
  from: string;
}

const same = (a: string, b: string | null | undefined): boolean => !!b && a.toLowerCase() === b.toLowerCase();

/** Received and owned: the note's owner is the pasted key's own pk. */
export function isOwned(note: RoleFields, ownPk: string | null | undefined): boolean {
  return note.role === 'received' && same(note.pk, ownPk);
}

/**
 * The row as the history may show it, or `null` to drop it. A received row whose owner is not
 * the key's pk is dropped; a sent row whose creator is not the key's pk is shown as
 * `sent_unverified`, never as sent. Anything else passes unchanged.
 */
export function admit<N extends RoleFields>(note: N, ownPk: string | null | undefined): N | null {
  if (note.role === 'received') return same(note.pk, ownPk) ? note : null;
  if (note.role === 'sent' && !same(note.from, ownPk)) return { ...note, role: 'sent_unverified' };
  return note;
}

/** The notes a spendable balance may sum: received, owned by `ownPk`, and known unspent. */
export function spendableNotes<N extends RoleFields>(
  rows: { note: N; spent: { state: string } }[],
  ownPk: string | null | undefined
): N[] {
  return rows.filter((r) => isOwned(r.note, ownPk) && r.spent.state === 'unspent').map((r) => r.note);
}
