import { describe, expect, it } from 'vitest'
import {
  cameraState,
  heroProgress,
  remap,
  saturate,
  smoothstep,
} from '@/lib/scene/progress'
import { camera, scroll } from '@/lib/scene/config'

describe('saturate', () => {
  it('clamps both ends and passes the middle through', () => {
    expect(saturate(-3)).toBe(0)
    expect(saturate(0.42)).toBe(0.42)
    expect(saturate(9)).toBe(1)
  })
})

describe('remap', () => {
  it('normalises a range', () => {
    expect(remap(5, 0, 10)).toBe(0.5)
    expect(remap(-1, 0, 10)).toBe(0)
    expect(remap(11, 0, 10)).toBe(1)
  })

  it('returns 0 for an empty range instead of dividing by zero', () => {
    expect(remap(5, 3, 3)).toBe(0)
  })
})

describe('smoothstep', () => {
  it('pins the endpoints and passes through the midpoint', () => {
    expect(smoothstep(0)).toBe(0)
    expect(smoothstep(1)).toBe(1)
    expect(smoothstep(0.5)).toBeCloseTo(0.5)
  })

  it('flattens near the ends, which is the whole reason it is here', () => {
    // The derivative is zero at both ends, so a small step in from 0 moves
    // the output much less than the linear amount.
    expect(smoothstep(0.1)).toBeLessThan(0.1)
    expect(smoothstep(0.9)).toBeGreaterThan(0.9)
  })
})

describe('heroProgress', () => {
  it('spans the configured number of screens', () => {
    const vh = 800
    const span = vh * scroll.screens
    expect(heroProgress(0, vh)).toBe(0)
    expect(heroProgress(span / 2, vh)).toBeCloseTo(0.5)
    expect(heroProgress(span, vh)).toBe(1)
  })

  it('clamps past the end rather than running on', () => {
    expect(heroProgress(99999, 800)).toBe(1)
  })

  it('survives a zero-height viewport', () => {
    // Happens for one frame on some mobile browsers during an orientation
    // change, and a NaN here would poison the camera matrix for the session.
    expect(heroProgress(100, 0)).toBe(0)
  })
})

describe('cameraState', () => {
  it('starts and ends on the configured positions', () => {
    expect(cameraState(0).position).toEqual([...camera.startPosition])
    const end = cameraState(1).position
    expect(end[0]).toBeCloseTo(camera.endPosition[0])
    expect(end[1]).toBeCloseTo(camera.endPosition[1])
    expect(end[2]).toBeCloseTo(camera.endPosition[2])
  })

  it('moves forward monotonically', () => {
    // Explicit values rather than a loop over config: the point is that the
    // camera never backs up, whatever the artistic numbers become.
    let previousZ = Infinity
    for (let p = 0; p <= 1.0001; p += 0.05) {
      const z = cameraState(p).position[2]
      expect(z).toBeLessThanOrEqual(previousZ + 1e-9)
      previousZ = z
    }
  })

  it('holds the hero at full opacity until the dissolve begins', () => {
    expect(cameraState(0).opacity).toBe(1)
    expect(cameraState(scroll.dofStart).opacity).toBe(1)
    expect(cameraState(scroll.fadeEnd).opacity).toBe(0)
    expect(cameraState(1).opacity).toBe(0)
  })

  it('keeps depth of field out of the hero proper', () => {
    expect(cameraState(0).dof).toBe(0)
    expect(cameraState(scroll.dofStart).dof).toBe(0)
    expect(cameraState(1).dof).toBe(1)
  })

  it('has the cloud absent before the entry point', () => {
    expect(cameraState(0).insideCloud).toBe(0)
    expect(cameraState(scroll.particleEntry).insideCloud).toBe(0)
    expect(cameraState(1).insideCloud).toBe(1)
  })
})
