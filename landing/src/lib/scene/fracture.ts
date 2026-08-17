/**
 * Building the geometry of one quarter of the fractured cube.
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

import { FRACTURE, CUBE } from './config'
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

type Face = {
  /** The face's corner in whole-cube coordinates. */
  origin: [number, number, number]
  /** The two vectors spanning the face. */
  u: [number, number, number]
  v: [number, number, number]
  /** Outward for outer faces, into the gap for fracture surfaces. */
  normal: [number, number, number]
  cut: boolean
}

function faces(quadrant: Quadrant, half: number): Face[] {
  const { sx, sy } = quadrant
  const h = half
  const x0 = 0
  const x1 = sx * h
  const y0 = 0
  const y1 = sy * h

  return [
    // Outer face along X.
    {
      origin: [x1, y0, -h],
      u: [0, y1 - y0, 0],
      v: [0, 0, 2 * h],
      normal: [sx, 0, 0],
      cut: false,
    },
    // Outer face along Y.
    {
      origin: [x0, y1, -h],
      u: [x1 - x0, 0, 0],
      v: [0, 0, 2 * h],
      normal: [0, sy, 0],
      cut: false,
    },
    // Outer faces along Z — the lid and the floor.
    {
      origin: [x0, y0, h],
      u: [x1 - x0, 0, 0],
      v: [0, y1 - y0, 0],
      normal: [0, 0, 1],
      cut: false,
    },
    {
      origin: [x0, y0, -h],
      u: [x1 - x0, 0, 0],
      v: [0, y1 - y0, 0],
      normal: [0, 0, -1],
      cut: false,
    },
    // Fracture surface on the plane X = 0: faces into the gap, i.e. away from
    // the part.
    {
      origin: [x0, y0, -h],
      u: [0, y1 - y0, 0],
      v: [0, 0, 2 * h],
      normal: [-sx, 0, 0],
      cut: true,
    },
    // Fracture surface on the plane Y = 0.
    {
      origin: [x0, y0, -h],
      u: [x1 - x0, 0, 0],
      v: [0, 0, 2 * h],
      normal: [0, -sy, 0],
      cut: true,
    },
  ]
}

function cross(
  a: [number, number, number],
  b: [number, number, number]
): [number, number, number] {
  return [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ]
}

function dot(a: [number, number, number], b: [number, number, number]): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/**
 * Builds one quarter.
 *
 * Faces do not share vertices with one another: the cube has to have sharp
 * edges, and a chip only reads as a facet with a normal of its own. So normals
 * are averaged within a face and never across its boundary.
 */
export function buildPart(quadrant: Quadrant, subdivision: number): PartGeometry {
  const half = CUBE.size / 2
  const n = Math.max(1, Math.floor(subdivision))
  const perFaceVerts = (n + 1) * (n + 1)
  const list = faces(quadrant, half)

  const vertexCount = perFaceVerts * list.length
  const triangleCount = n * n * 2 * list.length

  const position = new Float32Array(vertexCount * 3)
  const normal = new Float32Array(vertexCount * 3)
  const rest = new Float32Array(vertexCount * 3)
  const cut = new Float32Array(vertexCount)
  const index = new Uint32Array(triangleCount * 3)

  let vertexBase = 0
  let indexCursor = 0
  const displaced: [number, number, number] = [0, 0, 0]

  for (const face of list) {
    // The winding is chosen so that u × v matches the declared normal —
    // together with the triangle order below, that is what makes the normals
    // point outward.
    let u = face.u
    let v = face.v
    if (dot(cross(u, v), face.normal) < 0) {
      const swap = u
      u = v
      v = swap
    }

    for (let iv = 0; iv <= n; iv += 1) {
      const tv = iv / n
      for (let iu = 0; iu <= n; iu += 1) {
        const tu = iu / n
        const rx = face.origin[0] + u[0] * tu + v[0] * tv
        const ry = face.origin[1] + u[1] * tu + v[1] * tv
        const rz = face.origin[2] + u[2] * tu + v[2] * tv

        displaceVertex(rx, ry, rz, half, displaced)

        const slot = (vertexBase + iv * (n + 1) + iu) * 3
        position[slot] = displaced[0]
        position[slot + 1] = displaced[1]
        position[slot + 2] = displaced[2]
        rest[slot] = rx
        rest[slot + 1] = ry
        rest[slot + 2] = rz
        cut[vertexBase + iv * (n + 1) + iu] = face.cut ? 1 : 0
      }
    }

    for (let iv = 0; iv < n; iv += 1) {
      for (let iu = 0; iu < n; iu += 1) {
        const a = vertexBase + iv * (n + 1) + iu
        const b = a + 1
        const c = a + (n + 1)
        const d = c + 1
        // The order has to produce the normal `u × v` and not its opposite:
        // (a,b,c) gives (b−a) × (c−a) = u × v. The reverse order would turn
        // every face inside out — outer faces would become back-facing and be
        // culled, fracture surfaces would become front-facing, and the stone
        // would look hollow.
        index[indexCursor] = a
        index[indexCursor + 1] = b
        index[indexCursor + 2] = c
        index[indexCursor + 3] = b
        index[indexCursor + 4] = d
        index[indexCursor + 5] = c
        indexCursor += 6
      }
    }

    vertexBase += perFaceVerts
  }

  computeNormals(position, index, normal)

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
