/**
 * Building the geometry of the shards of the fractured cube.
 *
 * Pure functions over typed arrays: no three.js and no React here, so all of
 * it is checkable by test rather than by eye.
 *
 * ## Why the parts mate jag for jag
 *
 * A vertex is displaced by `D(r)`, where `r` is its position **in the
 * coordinates of the whole cube, before the cut**, and `D` is the vector field
 * from `noise.ts`. The displacement does not depend on which part the vertex
 * belongs to, nor on the face normal, nor on anything else.
 *
 * It follows that two coincident vertices of neighbouring parts have the same
 * `r`, therefore receive the same `D(r)`, therefore remain coincident. The
 * mating surface is not "fitted" — it is one and the same surface, computed
 * twice.
 *
 * The same reasoning explains why the displacement cannot follow the normal:
 * the two sides of a cut have opposite normals, and displacing along them
 * would either pull the surfaces apart or push one into the other.
 */

import { FRACTURE, CUBE, SHATTER } from './config'
import { displacement3, fbm3 } from './noise'

export type Quadrant = {
  /** Sign along X: +1 or -1. */
  sx: 1 | -1
  /** Sign along Y: +1 or -1. */
  sy: 1 | -1
}

/** The four parts in reading order: top-left, top-right, bottom-left, bottom-right. */
export const QUADRANTS: readonly Quadrant[] = [
  { sx: -1, sy: 1 },
  { sx: 1, sy: 1 },
  { sx: -1, sy: -1 },
  { sx: 1, sy: -1 },
]

/**
 * The direction a part moves away from the centre — for the resting gap and
 * for the lift under the cursor alike.
 *
 * The pure diagonal `(sx, sy, 0)` is the only direction that carries the part
 * away from **both** cut planes at once, which makes it impossible for parts
 * to intersect at any amplitude.
 *
 * The brief asked for the parts to "rise", so the diagonal is mixed with `+Y`.
 * That mixing has a hard ceiling: for the lower parts `+Y` reduces the
 * clearance from the plane `Y = 0`, and at `bias = 1 / (1 + √2) ≈ 0.4142` the
 * clearance reaches zero — the part starts to *slide along* the cut plane, and
 * sliding across a jagged surface means the jags pierce each other. Past that
 * limit the sign flips and the part drives into its neighbour.
 *
 * So the limit is not a place to stand, but a place not to reach: the clamp
 * leaves headroom.
 */
export const MAX_UP_BIAS = 1 / (1 + Math.SQRT2)

const UP_BIAS_MARGIN = 0.9

export function partDirection(
  quadrant: Quadrant,
  upBias: number
): [number, number, number] {
  const bias = Math.min(Math.max(upBias, 0), MAX_UP_BIAS * UP_BIAS_MARGIN)
  const diagonal = 1 / Math.SQRT2

  const x = quadrant.sx * diagonal * (1 - bias)
  const y = quadrant.sy * diagonal * (1 - bias) + bias
  const length = Math.hypot(x, y)

  return length > 0 ? [x / length, y / length, 0] : [0, 0, 0]
}

export type PartGeometry = {
  position: Float32Array
  normal: Float32Array
  /** The vertex position in whole-cube coordinates, before displacement. */
  rest: Float32Array
  /** 1 for a vertex on a fracture surface, 0 for one on the outer surface. */
  cut: Float32Array
  index: Uint32Array
  triangles: number
}

function smoothstep(edge0: number, edge1: number, x: number): number {
  if (edge1 === edge0) return x < edge0 ? 0 : 1
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)))
  return t * t * (3 - 2 * t)
}

/**
 * How close a point is to an edge and to a corner of the cube, in [0,1].
 *
 * A point lies on an edge when two of its coordinates are close to the
 * half-extent in absolute value, and on a corner when three are. So it is
 * enough to sort the |coordinates| descending: the second element accounts for
 * the edge, the third for the corner.
 */
export function edgeProximity(
  x: number,
  y: number,
  z: number,
  half: number,
  width: number
): { edge: number; corner: number } {
  const a = [Math.abs(x) / half, Math.abs(y) / half, Math.abs(z) / half].sort(
    (p, q) => q - p
  )
  const from = 1 - width
  return {
    edge: smoothstep(from, 1, a[1]),
    corner: smoothstep(from, 1, a[2]),
  }
}

const scratch: [number, number, number] = [0, 0, 0]

/**
 * Vertex displacement. The single function that defines the stone's shape —
 * and the single place to look if the parts have drifted apart.
 */
export function displaceVertex(
  x: number,
  y: number,
  z: number,
  half: number,
  out: [number, number, number] = [0, 0, 0]
): [number, number, number] {
  const { seed, frequency, amplitude, fbm, edge, seam } = FRACTURE

  displacement3(x * frequency, y * frequency, z * frequency, seed, fbm, scratch)

  const { edge: onEdge, corner: onCorner } = edgeProximity(x, y, z, half, edge.width)

  // Chips are a separate field at a lower frequency. At the same frequency
  // they would not be chips but amplified grain: the silhouette would go
  // fuzzy rather than chipped.
  const chipWeight = (edge.gain - 1) * onEdge + edge.cornerGain * onCorner
  const chipFrequency = frequency * edge.frequencyScale

  out[0] = scratch[0]
  out[1] = scratch[1]
  out[2] = scratch[2]

  if (chipWeight > 0) {
    const cx = x * chipFrequency
    const cy = y * chipFrequency
    const cz = z * chipFrequency
    out[0] += fbm3(cx, cy, cz, seed + 4111, fbm) * chipWeight
    out[1] += fbm3(cx, cy, cz, seed + 8221, fbm) * chipWeight
    out[2] += fbm3(cx, cy, cz, seed + 12331, fbm) * chipWeight
  }

  // Seam meander.
  //
  // The displacement here is **directed** rather than spread over three axes,
  // and that matters more than its amplitude. The fissure along the plane
  // X = 0 shows on a face as a line, and only motion along X — the plane's own
  // normal — pulls it off straight. Isotropic noise would give sideways
  // wandering only a third of its amplitude and spend the rest moving along
  // the fissure, where it cannot be seen.
  //
  // The two planes are computed separately and summed, so there is no
  // branching: choosing between "we are near the X plane or the Y plane" would
  // be discontinuous along the diagonal and leave a crease at the centre of
  // the cross.
  const nearX = 1 - smoothstep(0, seam.width, Math.abs(x) / half)
  const nearY = 1 - smoothstep(0, seam.width, Math.abs(y) / half)

  if (nearX > 0 || nearY > 0) {
    const sx = x * frequency * seam.frequencyScale
    const sy = y * frequency * seam.frequencyScale
    const sz = z * frequency * seam.frequencyScale

    out[0] += fbm3(sx, sy, sz, seed + 20441, fbm) * seam.gain * nearX
    out[1] += fbm3(sx, sy, sz, seed + 24551, fbm) * seam.gain * nearY

    // A little along the fissure too, so its lip is not level in height.
    out[2] +=
      fbm3(sx, sy, sz, seed + 28661, fbm) *
      seam.gain *
      seam.alongShare *
      Math.max(nearX, nearY)
  }

  out[0] = x + out[0] * amplitude
  out[1] = y + out[1] * amplitude
  out[2] = z + out[2] * amplitude
  return out
}

export type Shard = {
  /** The quarter the shard belongs to: it opens together with that quarter. */
  quadrant: Quadrant
  /** Opposite corners of the shard, in whole-cube coordinates before displacement. */
  min: [number, number, number]
  max: [number, number, number]
}

/**
 * The cube cut into boxes by the planes of `SHATTER.cuts`.
 *
 * The planes run through the whole cube, so two neighbours always share an
 * entire face: there are no T-junctions, and the argument at the top of this
 * file covers every cut, not only the two the quarters open along.
 */
export function shardGrid(): Shard[] {
  const half = CUBE.size / 2
  const bounds = ([0, 1, 2] as const).map((axis) => [
    -half,
    ...SHATTER.cuts[axis].map((cut) => cut * half),
    half,
  ])

  const grid: Shard[] = []
  for (let iz = 0; iz + 1 < bounds[2].length; iz += 1) {
    for (let iy = 0; iy + 1 < bounds[1].length; iy += 1) {
      for (let ix = 0; ix + 1 < bounds[0].length; ix += 1) {
        const min: Shard['min'] = [bounds[0][ix], bounds[1][iy], bounds[2][iz]]
        const max: Shard['max'] = [bounds[0][ix + 1], bounds[1][iy + 1], bounds[2][iz + 1]]
        const sx = min[0] + max[0] > 0 ? 1 : -1
        const sy = min[1] + max[1] > 0 ? 1 : -1
        const quadrant = QUADRANTS.find((q) => q.sx === sx && q.sy === sy)
        if (quadrant) grid.push({ quadrant, min, max })
      }
    }
  }
  return grid
}

/**
 * Builds one shard.
 *
 * Every vertex sits on one lattice shared by the whole cube: `2n` steps across
 * X and Y, `n` across Z. Shard bounds snap to it, so coincident vertices of two
 * shards are computed from the same integers and come out bit-identical.
 *
 * Faces do not share vertices with one another: the cube has to have sharp
 * edges, and a chip only reads as a facet with a normal of its own. Within one
 * plane the surface does continue into the neighbouring shard, so each face is
 * built one lattice step wider than it is, normals are taken there, and the
 * extra ring is dropped. Two shards then shade a closed cut identically.
 */
export function buildShard(shard: Shard, subdivision: number): PartGeometry {
  const half = CUBE.size / 2
  const n = Math.max(1, Math.floor(subdivision))
  const count = [2 * n, 2 * n, n]
  const step = [half / n, half / n, (2 * half) / n]
  const lo = shard.min.map((v, axis) => Math.round((v + half) / step[axis]))
  const hi = shard.max.map((v, axis) => Math.round((v + half) / step[axis]))

  const faces = ([0, 1, 2] as const).flatMap((axis) =>
    ([-1, 1] as const).map((side) => {
      // e_u × e_v has to equal the face normal `side · e_axis`.
      const u = (axis + (side > 0 ? 1 : 2)) % 3
      const v = (axis + (side > 0 ? 2 : 1)) % 3
      const plane = side > 0 ? hi[axis] : lo[axis]
      return {
        axis,
        u,
        v,
        plane,
        cut: plane !== 0 && plane !== count[axis],
        // The ring of extra vertices, where the plane goes on past the shard.
        u0: lo[u] - (lo[u] > 0 ? 1 : 0),
        u1: hi[u] + (hi[u] < count[u] ? 1 : 0),
        v0: lo[v] - (lo[v] > 0 ? 1 : 0),
        v1: hi[v] + (hi[v] < count[v] ? 1 : 0),
      }
    })
  )

  let vertexCount = 0
  let triangleCount = 0
  for (const face of faces) {
    vertexCount += (hi[face.u] - lo[face.u] + 1) * (hi[face.v] - lo[face.v] + 1)
    triangleCount += (hi[face.u] - lo[face.u]) * (hi[face.v] - lo[face.v]) * 2
  }

  const position = new Float32Array(vertexCount * 3)
  const normal = new Float32Array(vertexCount * 3)
  const rest = new Float32Array(vertexCount * 3)
  const cut = new Float32Array(vertexCount)
  const index = new Uint32Array(triangleCount * 3)

  let vertexBase = 0
  let indexCursor = 0
  const displaced: [number, number, number] = [0, 0, 0]
  const point: [number, number, number] = [0, 0, 0]

  for (const face of faces) {
    const wideU = face.u1 - face.u0 + 1
    const wideV = face.v1 - face.v0 + 1
    const widePosition = new Float32Array(wideU * wideV * 3)
    const wideNormal = new Float32Array(wideU * wideV * 3)
    const wideIndex = new Uint32Array((wideU - 1) * (wideV - 1) * 6)

    point[face.axis] = -half + face.plane * step[face.axis]
    for (let iv = face.v0; iv <= face.v1; iv += 1) {
      for (let iu = face.u0; iu <= face.u1; iu += 1) {
        point[face.u] = -half + iu * step[face.u]
        point[face.v] = -half + iv * step[face.v]
        displaceVertex(point[0], point[1], point[2], half, displaced)
        widePosition.set(displaced, ((iv - face.v0) * wideU + (iu - face.u0)) * 3)
      }
    }

    // (a,b,c) gives (b−a) × (c−a) = u × v, the face normal. The reverse order
    // would turn every face inside out and the stone would look hollow.
    const quad = (out: Uint32Array, cursor: number, a: number, row: number) => {
      out[cursor] = a
      out[cursor + 1] = a + 1
      out[cursor + 2] = a + row
      out[cursor + 3] = a + 1
      out[cursor + 4] = a + row + 1
      out[cursor + 5] = a + row
    }

    let wideCursor = 0
    for (let iv = 0; iv + 1 < wideV; iv += 1) {
      for (let iu = 0; iu + 1 < wideU; iu += 1) {
        quad(wideIndex, wideCursor, iv * wideU + iu, wideU)
        wideCursor += 6
      }
    }
    computeNormals(widePosition, wideIndex, wideNormal)

    const nu = hi[face.u] - lo[face.u]
    const nv = hi[face.v] - lo[face.v]
    for (let iv = 0; iv <= nv; iv += 1) {
      for (let iu = 0; iu <= nu; iu += 1) {
        const vertex = vertexBase + iv * (nu + 1) + iu
        const wide =
          ((iv + lo[face.v] - face.v0) * wideU + (iu + lo[face.u] - face.u0)) * 3
        position.set(widePosition.subarray(wide, wide + 3), vertex * 3)
        normal.set(wideNormal.subarray(wide, wide + 3), vertex * 3)
        rest[vertex * 3 + face.axis] = point[face.axis]
        rest[vertex * 3 + face.u] = -half + (iu + lo[face.u]) * step[face.u]
        rest[vertex * 3 + face.v] = -half + (iv + lo[face.v]) * step[face.v]
        cut[vertex] = face.cut ? 1 : 0
        if (iu < nu && iv < nv) {
          quad(index, indexCursor, vertex, nu + 1)
          indexCursor += 6
        }
      }
    }

    vertexBase += (nu + 1) * (nv + 1)
  }

  return { position, normal, rest, cut, index, triangles: triangleCount }
}

/** Normals from triangle areas. Area weighting is free and more accurate. */
function computeNormals(
  position: Float32Array,
  index: Uint32Array,
  out: Float32Array
): void {
  out.fill(0)

  for (let i = 0; i < index.length; i += 3) {
    const ia = index[i] * 3
    const ib = index[i + 1] * 3
    const ic = index[i + 2] * 3

    const e1x = position[ib] - position[ia]
    const e1y = position[ib + 1] - position[ia + 1]
    const e1z = position[ib + 2] - position[ia + 2]
    const e2x = position[ic] - position[ia]
    const e2y = position[ic + 1] - position[ia + 1]
    const e2z = position[ic + 2] - position[ia + 2]

    const nx = e1y * e2z - e1z * e2y
    const ny = e1z * e2x - e1x * e2z
    const nz = e1x * e2y - e1y * e2x

    for (const slot of [ia, ib, ic]) {
      out[slot] += nx
      out[slot + 1] += ny
      out[slot + 2] += nz
    }
  }

  for (let i = 0; i < out.length; i += 3) {
    const length = Math.hypot(out[i], out[i + 1], out[i + 2])
    if (length > 0) {
      out[i] /= length
      out[i + 1] /= length
      out[i + 2] /= length
    }
  }
}
