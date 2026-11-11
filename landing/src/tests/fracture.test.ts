import { describe, expect, it } from 'vitest'

import { CUBE, FRACTURE, HOVER } from '../lib/scene/config'
import {
  MAX_UP_BIAS,
  QUADRANTS,
  buildShard,
  displaceVertex,
  edgeProximity,
  partDirection,
  shardGrid,
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
describe('interlocking of the shards', () => {
  const SUBDIVISION = 12
  const built = shardGrid().map((shard) => buildShard(shard, SUBDIVISION))

  it('cuts the cube into shards that keep the four quarters', () => {
    const grid = shardGrid()
    expect(grid.length).toBeGreaterThan(QUADRANTS.length * 4)
    for (const quadrant of QUADRANTS) {
      expect(grid.some((shard) => shard.quadrant === quadrant)).toBe(true)
    }
    for (const shard of grid) {
      // No shard straddles the planes X = 0 and Y = 0: the quarters still
      // open along them.
      expect(shard.min[0] * shard.max[0]).toBeGreaterThanOrEqual(0)
      expect(shard.min[1] * shard.max[1]).toBeGreaterThanOrEqual(0)
    }
  })

  it('makes neighbours coincide on every shared surface', () => {
    const seen = new Map<string, [number, number, number]>()
    let shared = 0
    for (const part of built) {
      for (let i = 0; i < part.rest.length; i += 3) {
        const key = [part.rest[i], part.rest[i + 1], part.rest[i + 2]].join('|')
        const position: [number, number, number] = [
          part.position[i],
          part.position[i + 1],
          part.position[i + 2],
        ]
        const other = seen.get(key)
        if (other) {
          shared += 1
          expect(position).toEqual(other)
        } else {
          seen.set(key, position)
        }
      }
    }

    // If no shared points were found, the test checked nothing.
    expect(shared).toBeGreaterThan(built.length * SUBDIVISION)
  })

  /**
   * At rest the cuts between shards of one quarter are closed, so the outer
   * surface has to shade as one piece. Normals taken from each shard's own
   * triangles alone would differ along every cut and draw the grid on the
   * stone.
   */
  it('shades the outer surface continuously across a closed cut', () => {
    const seen = new Map<string, [number, number, number]>()
    let shared = 0
    for (const part of built) {
      for (let i = 0; i < part.rest.length; i += 3) {
        if (part.cut[i / 3] === 1) continue
        const rest = [part.rest[i], part.rest[i + 1], part.rest[i + 2]]
        // Cube edges and corners are sharp on purpose: only face interiors count.
        if (rest.filter((v) => Math.abs(v) === HALF).length !== 1) continue
        const key = rest.join('|')
        const normal: [number, number, number] = [
          part.normal[i],
          part.normal[i + 1],
          part.normal[i + 2],
        ]
        const other = seen.get(key)
        if (other) {
          shared += 1
          for (let axis = 0; axis < 3; axis += 1) {
            expect(normal[axis]).toBeCloseTo(other[axis], 5)
          }
        } else {
          seen.set(key, normal)
        }
      }
    }
    expect(shared).toBeGreaterThan(SUBDIVISION)
  })
})

describe('building a shard', () => {
  const SUBDIVISION = 8
  const grid = shardGrid()
  const inner = (shard: (typeof grid)[number]) =>
    [...shard.min, ...shard.max].every((v) => Math.abs(v) < HALF)
  const cornerShard = grid.find(
    (shard) =>
      [0, 1, 2].every((a) => shard.min[a] === -HALF || shard.max[a] === HALF)
  )!
  const part = buildShard(cornerShard, SUBDIVISION)

  it('marks every face of a buried shard as a fracture surface', () => {
    const buried = buildShard(grid.find(inner)!, SUBDIVISION)
    expect(buried.cut.every((v) => v === 1)).toBe(true)
  })

  it('marks only faces on the cube surface as outer', () => {
    let outer = 0
    for (let i = 0; i < part.cut.length; i += 1) {
      if (part.cut[i] === 1) continue
      outer += 1
      const rest = [part.rest[i * 3], part.rest[i * 3 + 1], part.rest[i * 3 + 2]]
      expect(rest.some((v) => Math.abs(v) === HALF)).toBe(true)
    }
    expect(outer).toBeGreaterThan(0)
    expect(outer).toBeLessThan(part.cut.length)
  })

  it('produces unit normals', () => {
    for (let i = 0; i < part.normal.length; i += 3) {
      const length = Math.hypot(part.normal[i], part.normal[i + 1], part.normal[i + 2])
      expect(length).toBeCloseTo(1, 5)
    }
  })

  /**
   * Winding. Every face has to look away from its own shard: outer faces
   * outward, fracture surfaces into the gap. A reversed winding does not crash
   * and does not paint anything red: the faces are simply culled as
   * back-facing and the stone looks hollow. Which is exactly what happened.
   */
  it.each(grid.map((shard, index) => [index, shard] as const))(
    'orients every face of shard %i away from it',
    (_index, shard) => {
      const built = buildShard(shard, 6)
      const centre = [0, 1, 2].map((a) => (shard.min[a] + shard.max[a]) / 2)
      const at = (vertex: number, axis: number) => built.position[vertex * 3 + axis]

      for (let i = 0; i < built.index.length; i += 3) {
        const [a, b, c] = [built.index[i], built.index[i + 1], built.index[i + 2]]
        const e1 = [0, 1, 2].map((k) => at(b, k) - at(a, k))
        const e2 = [0, 1, 2].map((k) => at(c, k) - at(a, k))
        const normal = [
          e1[1] * e2[2] - e1[2] * e2[1],
          e1[2] * e2[0] - e1[0] * e2[2],
          e1[0] * e2[1] - e1[1] * e2[0],
        ]
        // The triangle's vertex normals follow its winding, and the relief
        // rocks them only a little: all three have to agree with it.
        for (const vertex of [a, b, c]) {
          const dot =
            normal[0] * built.normal[vertex * 3] +
            normal[1] * built.normal[vertex * 3 + 1] +
            normal[2] * built.normal[vertex * 3 + 2]
          expect(dot).toBeGreaterThan(0)
        }
      }

      // And the faces as a whole look away from the shard's centre.
      let away = 0
      for (let vertex = 0; vertex < built.rest.length / 3; vertex += 1) {
        for (let k = 0; k < 3; k += 1) {
          away += built.normal[vertex * 3 + k] * (built.rest[vertex * 3 + k] - centre[k])
        }
      }
      expect(away).toBeGreaterThan(0)
    }
  )

  it('references only existing vertices', () => {
    const vertices = part.rest.length / 3
    expect(part.index.length).toBe(part.triangles * 3)
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
