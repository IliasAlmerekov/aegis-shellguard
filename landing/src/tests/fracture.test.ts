import { describe, expect, it } from 'vitest'

import { CUBE, FRACTURE, HOVER } from '../lib/scene/config'
import {
  MAX_UP_BIAS,
  QUADRANTS,
  buildPart,
  displaceVertex,
  edgeProximity,
  partDirection,
} from '../lib/scene/fracture'
import { fbm3, noise3 } from '../lib/scene/noise'

const HALF = CUBE.size / 2

describe('noise', () => {
  it('is deterministic for one seed', () => {
    expect(noise3(0.31, -1.7, 4.2, 9)).toBe(noise3(0.31, -1.7, 4.2, 9))
  })

  it('differs across seeds', () => {
    expect(noise3(0.31, -1.7, 4.2, 9)).not.toBe(noise3(0.31, -1.7, 4.2, 10))
  })

  it('lies in [0,1)', () => {
    for (let i = 0; i < 4000; i += 1) {
      const v = noise3(i * 0.137, i * -0.61, i * 0.29, 3)
      expect(v).toBeGreaterThanOrEqual(0)
      expect(v).toBeLessThan(1)
    }
  })

  it('gives fbm in [-1,1]', () => {
    for (let i = 0; i < 4000; i += 1) {
      const v = fbm3(i * 0.091, i * 0.37, i * -0.13, 11, FRACTURE.fbm)
      expect(v).toBeGreaterThanOrEqual(-1)
      expect(v).toBeLessThanOrEqual(1)
    }
  })
})

describe('edge proximity', () => {
  it('is zero at the centre of the cube', () => {
    const { edge, corner } = edgeProximity(0, 0, 0, HALF, FRACTURE.edge.width)
    expect(edge).toBe(0)
    expect(corner).toBe(0)
  })

  it('is one on both counters at a corner', () => {
    const { edge, corner } = edgeProximity(HALF, -HALF, HALF, HALF, FRACTURE.edge.width)
    expect(edge).toBe(1)
    expect(corner).toBe(1)
  })

  it('reports an edge but no corner mid-edge', () => {
    const { edge, corner } = edgeProximity(HALF, HALF, 0, HALF, FRACTURE.edge.width)
    expect(edge).toBe(1)
    expect(corner).toBe(0)
  })

  it('reports neither edge nor corner mid-face', () => {
    const { edge, corner } = edgeProximity(HALF, 0, 0, HALF, FRACTURE.edge.width)
    expect(edge).toBe(0)
    expect(corner).toBe(0)
  })
})

describe('vertex displacement', () => {
  it('depends on nothing but the point', () => {
    const a = displaceVertex(0.2, -0.44, 0.71, HALF)
    const b = displaceVertex(0.2, -0.44, 0.71, HALF)
    expect(a).toEqual(b)
  })

  it('never moves a vertex further than amplitude, chips and meander allow', () => {
    const { amplitude, edge, seam } = FRACTURE
    const ceiling =
      amplitude * (1 + (edge.gain - 1) + edge.cornerGain + seam.gain)

    for (let i = 0; i < 3000; i += 1) {
      const x = (((i * 37) % 101) / 100) * CUBE.size - HALF
      const y = (((i * 53) % 101) / 100) * CUBE.size - HALF
      const z = (((i * 71) % 101) / 100) * CUBE.size - HALF
      const [dx, dy, dz] = displaceVertex(x, y, z, HALF)
      expect(Math.abs(dx - x)).toBeLessThanOrEqual(ceiling + 1e-9)
      expect(Math.abs(dy - y)).toBeLessThanOrEqual(ceiling + 1e-9)
      expect(Math.abs(dz - z)).toBeLessThanOrEqual(ceiling + 1e-9)
    }
  })
})

/**
 * The central claim of the whole geometry: neighbouring parts mate jag for
 * jag. It is checked not by "looks about right" but bit for bit — for any
 * point falling into both parts, the displaced position must be identical.
 */
describe('interlocking of the parts', () => {
  const SUBDIVISION = 12

  function restMap(quadrantIndex: number): Map<string, [number, number, number]> {
    const part = buildPart(QUADRANTS[quadrantIndex], SUBDIVISION)
    const map = new Map<string, [number, number, number]>()
    for (let i = 0; i < part.rest.length; i += 3) {
      const key = [part.rest[i], part.rest[i + 1], part.rest[i + 2]].join('|')
      map.set(key, [part.position[i], part.position[i + 1], part.position[i + 2]])
    }
    return map
  }

  it.each([
    ['top-left and top-right', 0, 1],
    ['bottom-left and bottom-right', 2, 3],
    ['top-left and bottom-left', 0, 2],
    ['top-right and bottom-right', 1, 3],
  ])('%s coincide on their shared surface', (_name, a, b) => {
    const first = restMap(a)
    const second = restMap(b)

    let shared = 0
    for (const [key, position] of first) {
      const other = second.get(key)
      if (!other) continue
      shared += 1
      expect(position).toEqual(other)
    }

    // If no shared points were found, the test checked nothing.
    expect(shared).toBeGreaterThan(SUBDIVISION)
  })
})

describe('building a quarter', () => {
  const SUBDIVISION = 8
  const part = buildPart(QUADRANTS[0], SUBDIVISION)

  it('counts six faces with no shared vertices', () => {
    const perFace = (SUBDIVISION + 1) * (SUBDIVISION + 1)
    expect(part.rest.length / 3).toBe(perFace * 6)
    expect(part.triangles).toBe(SUBDIVISION * SUBDIVISION * 2 * 6)
  })

  it('marks exactly two faces as fracture surfaces', () => {
    const perFace = (SUBDIVISION + 1) * (SUBDIVISION + 1)
    const marked = part.cut.reduce((sum, v) => sum + v, 0)
    expect(marked).toBe(perFace * 2)
  })

  it('produces unit normals', () => {
    for (let i = 0; i < part.normal.length; i += 3) {
      const length = Math.hypot(part.normal[i], part.normal[i + 1], part.normal[i + 2])
      expect(length).toBeCloseTo(1, 5)
    }
  })

  /**
   * Winding. Outer faces have to look outward, fracture surfaces into the gap.
   * A reversed winding does not crash and does not paint anything red: the
   * outer faces are simply culled as back-facing, the fracture surfaces become
   * front-facing, and the stone looks hollow. Which is exactly what happened.
   */
  it('orients the faces outward and the fracture into the gap', () => {
    const quadrant = QUADRANTS[1] // sx = +1, sy = +1
    const built = buildPart(quadrant, 6)
    const perFace = 7 * 7

    // Face order in `faces()`: outer X, outer Y, +Z, −Z, fracture X, fracture Y.
    const expected: [number, number, number][] = [
      [quadrant.sx, 0, 0],
      [0, quadrant.sy, 0],
      [0, 0, 1],
      [0, 0, -1],
      [-quadrant.sx, 0, 0],
      [0, -quadrant.sy, 0],
    ]

    expected.forEach((want, face) => {
      let sum = 0
      let count = 0
      for (let i = 0; i < perFace; i += 1) {
        const slot = (face * perFace + i) * 3
        sum +=
          built.normal[slot] * want[0] +
          built.normal[slot + 1] * want[1] +
          built.normal[slot + 2] * want[2]
        count += 1
      }
      // The relief rocks individual normals, so the average over the face is
      // what is checked: it has to point the declared way with ample margin.
      expect(sum / count).toBeGreaterThan(0.5)
    })
  })

  it('references only existing vertices', () => {
    const vertices = part.rest.length / 3
    for (const i of part.index) {
      expect(i).toBeLessThan(vertices)
    }
  })
})

/**
 * The lift direction has to carry a part away from both cut planes. Otherwise
 * the part slides along a jagged surface and the jags pierce its neighbour —
 * the only way to break the interlocking while leaving the geometry valid.
 */
describe('lift direction', () => {
  it('carries a part away from both cut planes at the configured bias', () => {
    for (const quadrant of QUADRANTS) {
      const [x, y] = partDirection(quadrant, HOVER.upBias)
      expect(x * quadrant.sx).toBeGreaterThan(0)
      expect(y * quadrant.sy).toBeGreaterThan(0)
    }
  })

  it('stays safe even when asked for a bias past the limit', () => {
    for (const quadrant of QUADRANTS) {
      const [x, y] = partDirection(quadrant, 10)
      expect(x * quadrant.sx).toBeGreaterThan(0)
      expect(y * quadrant.sy).toBeGreaterThan(0)
    }
  })

  it('keeps the configured bias inside the mathematical limit', () => {
    expect(HOVER.upBias).toBeLessThan(MAX_UP_BIAS)
  })

  it('is of unit length', () => {
    for (const quadrant of QUADRANTS) {
      const [x, y, z] = partDirection(quadrant, HOVER.upBias)
      expect(Math.hypot(x, y, z)).toBeCloseTo(1, 10)
    }
  })
})
