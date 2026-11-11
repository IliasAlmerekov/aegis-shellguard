import { describe, expect, it } from 'vitest'

import { BALANCE, CAMERA, CUBE } from '../lib/scene/config'
import { stoneLift, stoneScale } from '../lib/scene/fit'

/** The width of the frame at the stone's depth, in scene units. */
function frameWidth(aspect: number): number {
  const distance = Math.hypot(
    CAMERA.rest[0],
    CAMERA.rest[1] - CUBE.centerY,
    CAMERA.rest[2]
  )
  return 2 * distance * Math.tan((CAMERA.fov * Math.PI) / 360) * aspect
}

/** The width of the turned stone on screen at full size, in scene units. */
const STONE_WIDTH =
  CUBE.size * (Math.abs(Math.cos(BALANCE.pose[1])) + Math.abs(Math.sin(BALANCE.pose[1])))

describe('fitting the stone to the frame', () => {
  it('leaves the stone at full size on a desktop frame', () => {
    expect(stoneScale(16 / 9)).toBe(1)
    expect(stoneScale(4 / 3)).toBe(1)
  })

  it.each([
    ['375 × 812', 375 / 812],
    ['320 × 568', 320 / 568],
    ['390 × 844', 390 / 844],
  ])('keeps the stone inside a %s phone frame with a margin', (_name, aspect) => {
    const scale = stoneScale(aspect)
    expect(scale).toBeLessThan(1)
    expect(STONE_WIDTH * scale).toBeLessThanOrEqual(frameWidth(aspect) * CUBE.fit + 1e-9)
  })

  it('never grows the stone and never shrinks it away', () => {
    for (let aspect = 0.2; aspect <= 3; aspect += 0.05) {
      const scale = stoneScale(aspect)
      expect(scale).toBeLessThanOrEqual(1)
      expect(scale).toBeGreaterThan(0.2)
    }
  })

  it('lifts the stone only where it has been scaled down', () => {
    expect(stoneLift(16 / 9)).toBe(0)
    expect(stoneLift(4 / 3)).toBe(0)
    expect(stoneLift(375 / 812)).toBeGreaterThan(0)
    // The narrower the frame, the smaller the stone and the more room above it.
    expect(stoneLift(320 / 812)).toBeGreaterThan(stoneLift(375 / 812))
  })
})
