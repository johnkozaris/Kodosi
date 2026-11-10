const LEAST_ID = 256;
/** New ids stay below this, so that a program that numbers after the highest id has room. */
const ID_ROOM = 2 ** 30;

/**
 * A new id for a slide or a shape: a number that a file can hold, which none of `taken` is.
 * It comes at random, so that people who add at the same time take different ones.
 */
export function newId(taken: readonly string[], random: () => number = Math.random): string {
  for (;;) {
    const id = String(LEAST_ID + Math.floor(random() * (ID_ROOM - LEAST_ID)));
    if (!taken.includes(id)) return id;
  }
}
