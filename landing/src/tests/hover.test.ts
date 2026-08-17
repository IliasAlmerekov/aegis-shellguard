import { describe, expect, it } from 'vitest'

import { HOVER } from '../lib/scene/config'
import { QUADRANTS } from '../lib/scene/fracture'
import { approach, liftOffset, tauFor } from '../lib/scene/hover'

describe('approach toward a target', () => {
  it('does not move on a zero time step', () => {
    expect(approach(0.3, 1, 0, 0.3)).toBe(0.3)
  })

  it('never overshoots the target', () => {
    for (const dt of [0.001, 0.016, 0.05, 0.5, 10]) {
      const up = approach(0, 1, dt, 0.28)
      expect(up).toBeGreaterThanOrEqual(0)
      expect(up).toBeLessThanOrEqual(1)

      const down = approach(1, 0, dt, 0.55)
      expect(down).toBeGreaterThanOrEqual(0)
      expect(down).toBeLessThanOrEqual(1)
    }
  })

  it('settles into the socket instead of sticking beside it', () => {
    // The duration is expressed in `tau` rather than in frames: over 20 `tau`
    // the remainder falls by a factor of e²⁰ ≈ 5·10⁸ for any value from the
    // config. A fixed frame count would have made the test bogus at the first
    // edit to the release time — which is exactly what happened.
    const frames = Math.ceil(20 * HOVER.release * 60)
    let value = 1
    for (let step = 0; step < frames; step += 1) {
      value = approach(value, 0, 1 / 60, HOVER.release)
    }
    expect(value).toBeLessThan(1e-6)
  })

  /**
   * Frame-rate independence. The naive `current += (target - current) * k`
   * would make the gesture twice as fast at 144 Hz as at 60 — that is the
   * mistake this check guards against.
   */
  it('runs identically at different frame rates', () => {
    const seconds = 0.5
    const tau = 0.3

    const run = (fps: number) => {
      let value = 0
      for (let step = 0; step < fps * seconds; step += 1) {
        value = approach(value, 1, 1 / fps, tau)
      }
      return value
    }

    expect(run(144)).toBeCloseTo(run(60), 3)
    expect(run(30)).toBeCloseTo(run(60), 2)
  })

  it('releases more slowly than it attacks', () => {
    expect(tauFor(0, 1)).toBe(HOVER.attack)
    expect(tauFor(1, 0)).toBe(HOVER.release)
    expect(HOVER.release).toBeGreaterThan(HOVER.attack)
  })
})

describe('part offset', () => {
  it('is zero at rest', () => {
    for (const quadrant of QUADRANTS) {
      // Numerically, not via toBe and not by deep comparison: for negative
      // directions zero comes out as -0, and `Object.is(-0, 0)` is false. It is
      // the same zero, and the assertion about the offset is about magnitude,
      // not about the literal.
      for (const component of liftOffset(quadrant, 0)) {
        expect(Math.abs(component)).toBe(0)
      }
    }
  })

  it('equals the configured distance at full lift', () => {
    for (const quadrant of QUADRANTS) {
      const [x, y, z] = liftOffset(quadrant, 1)
      expect(Math.hypot(x, y, z)).toBeCloseTo(HOVER.lift, 10)
    }
  })

  /**
   * The same assertion as the one about direction, but stated through the
   * gesture itself: a part has to move away from both cut planes. Otherwise it
   * slides along a jagged surface and pierces its neighbour.
   */
  it('carries the part away from both cut planes', () => {
    for (const quadrant of QUADRANTS) {
      const [x, y] = liftOffset(quadrant, 1)
      expect(x * quadrant.sx).toBeGreaterThan(0)
      expect(y * quadrant.sy).toBeGreaterThan(0)
    }
  })
})
