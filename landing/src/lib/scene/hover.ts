/**
 * The lift of a part under the cursor.
 *
 * Pure gesture mathematics, kept away from the scene: convergence to the
 * socket and the absence of overshoot are properties a test checks, not
 * something eyes check. A miss here looks like a part that never quite settled
 * back into place, and that takes minutes to notice rather than seconds.
 */

import { HOVER } from './config'
import { partDirection, type Quadrant } from './fracture'

/**
 * One step of exponential approach toward a target.
 *
 * `tau` is the time in which the remainder shrinks by a factor of `e`. The
 * form is not chosen for elegance: it is **frame-rate independent**. The naive
 * `current += (target - current) * k` runs at a different speed at a different
 * frame rate, which would make the gesture twice as fast at 144 Hz as at 60.
 *
 * There is no overshoot by construction: the coefficient lies in [0,1), so the
 * sign of the remainder never flips. That is exactly the "easing into the
 * socket" the return was asked for — and it comes for free.
 */
export function approach(current: number, target: number, dt: number, tau: number): number {
  if (dt <= 0) return current
  if (tau <= 0) return target
  return current + (target - current) * (1 - Math.exp(-dt / tau))
}

/** Going out or settling back — the two directions have different times. */
export function tauFor(current: number, target: number): number {
  return target > current ? HOVER.attack : HOVER.release
}

/**
 * A part's offset at a given lift amount.
 *
 * The direction comes from `partDirection`, where it is also proved that it
 * carries the part away from both cut planes and therefore cannot drive it
 * into a neighbour.
 */
export function liftOffset(
  quadrant: Quadrant,
  amount: number,
  out: [number, number, number] = [0, 0, 0]
): [number, number, number] {
  const [dx, dy, dz] = partDirection(quadrant, HOVER.upBias)
  const distance = HOVER.lift * amount
  out[0] = dx * distance
  out[1] = dy * distance
  out[2] = dz * distance
  return out
}
