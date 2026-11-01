import type { Hinge } from "./nativeTabbar";

/** Past this the device is open flat: the peeked chat opens, and is read. */
export const PEEK_FLAT = 172;

/** The picked chat shows once the inner screen is on and the hinge is not yet
 *  flat. iOS turns the inner screen on part way open (about 110° in the
 *  simulator); the hinge status is no guide, as it flips between "closed"
 *  and "partial" while the hinge moves. */
export const peekOpen = (hinge: Hinge | null, wide: boolean) => wide && !!hinge && hinge.angle < PEEK_FLAT;

const ramp = (angle: number, from: number, to: number) => {
  const t = Math.min(1, Math.max(0, (angle - from) / (to - from)));
  return t * t * (3 - 2 * t);
};

/** 0 (hidden) to 1 (clear). `from` is the angle the peek first showed at: it
 *  starts blurred there, sharpens over the next 20°, and fades toward flat. */
export function peekClarity(angle: number, from: number) {
  return ramp(angle, from, from + 20) * (1 - ramp(angle, 145, PEEK_FLAT));
}
