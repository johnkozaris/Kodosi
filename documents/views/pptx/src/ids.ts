const LEAST_ID = 256;
// Cap below 2^30 so tools that allocate sequential IDs above the max still have headroom.
const ID_ROOM = 2 ** 30;

/** Picks a random unused ID so concurrent additions from different clients do not collide. */
export function newId(taken: readonly string[], random: () => number = Math.random): string {
  for (;;) {
    const id = String(LEAST_ID + Math.floor(random() * (ID_ROOM - LEAST_ID)));
    if (!taken.includes(id)) return id;
  }
}
