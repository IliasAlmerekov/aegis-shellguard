/**
 * Value noise in 3D, and fbm over it.
 *
 * Written here rather than pulled in as a dependency: exactly one kind of
 * noise is needed, it has to be deterministic for a given seed (otherwise the
 * geometry drifts between reloads and there is nothing for a test to assert),
 * and it has to run in TypeScript rather than GLSL — vertices are displaced
 * once, on the CPU, when the geometry is built.
 *
 * Value noise, not simplex: what separates them is the directional lattice
 * artefacts that show up on smooth organic forms. Here a photogrammetric
 * normal map sits on top of the noise and the form itself is stony and
 * angular, so there is nothing to reveal the value lattice — while it is half
 * the cost and its behaviour is easy to check by hand.
 */

/** Hash of three integers into [0,1). Deterministic, stateless. */
function hash(ix: number, iy: number, iz: number, seed: number): number {
  let h = ix * 374761393 + iy * 668265263 + iz * 2147483647 + seed * 1013904223
  h = (h ^ (h >>> 13)) >>> 0
  h = Math.imul(h, 1274126177) >>> 0
  h = (h ^ (h >>> 16)) >>> 0
  return h / 4294967296
}

/** Cubic smoothing curve: zero derivative at both ends. */
function fade(t: number): number {
  return t * t * (3 - 2 * t)
}

/** Value noise in [0,1). */
export function noise3(x: number, y: number, z: number, seed = 0): number {
  const ix = Math.floor(x)
  const iy = Math.floor(y)
  const iz = Math.floor(z)

  const fx = fade(x - ix)
  const fy = fade(y - iy)
  const fz = fade(z - iz)

  const c000 = hash(ix, iy, iz, seed)
  const c100 = hash(ix + 1, iy, iz, seed)
  const c010 = hash(ix, iy + 1, iz, seed)
  const c110 = hash(ix + 1, iy + 1, iz, seed)
  const c001 = hash(ix, iy, iz + 1, seed)
  const c101 = hash(ix + 1, iy, iz + 1, seed)
  const c011 = hash(ix, iy + 1, iz + 1, seed)
  const c111 = hash(ix + 1, iy + 1, iz + 1, seed)

  const x00 = c000 + (c100 - c000) * fx
  const x10 = c010 + (c110 - c010) * fx
  const x01 = c001 + (c101 - c001) * fx
  const x11 = c011 + (c111 - c011) * fx

  const y0 = x00 + (x10 - x00) * fy
  const y1 = x01 + (x11 - x01) * fy

  return y0 + (y1 - y0) * fz
}

export type FbmOptions = {
  /** Number of octaves. */
  octaves: number
  /** How much the frequency grows per octave. */
  lacunarity: number
  /** How much the contribution falls per octave. */
  gain: number
  /**
   * Share of the ridged component, 0..1.
   *
   * Smooth fbm gives waves — stone does not break that way. Ridged
   * (`1 - |2n-1|`) gives folds with sharp crests, which is what a fracture
   * looks like. The mix keeps the form somewhere between "melted" and
   * "shattered", and it is the one number that has to be dialled in by eye.
   */
  ridged: number
}

/** fbm in [-1,1]. */
export function fbm3(
  x: number,
  y: number,
  z: number,
  seed: number,
  options: FbmOptions
): number {
  const { octaves, lacunarity, gain, ridged } = options

  let amplitude = 1
  let frequency = 1
  let sum = 0
  let norm = 0

  for (let octave = 0; octave < octaves; octave += 1) {
    const n = noise3(x * frequency, y * frequency, z * frequency, seed + octave * 101)

    const smooth = n * 2 - 1
    const ridge = 1 - 2 * Math.abs(smooth)
    const value = smooth + (ridge - smooth) * ridged

    sum += value * amplitude
    norm += amplitude

    amplitude *= gain
    frequency *= lacunarity
  }

  return norm > 0 ? sum / norm : 0
}

/**
 * The displacement vector field at a point.
 *
 * Three independent fbm channels with different seeds. This is the field the
 * vertices are displaced by: it depends on **the point alone**, so two
 * coincident vertices of neighbouring parts receive the same displacement and
 * the mating surfaces stay in exact contact. The property holds by
 * construction, not by tuning.
 */
export function displacement3(
  x: number,
  y: number,
  z: number,
  seed: number,
  options: FbmOptions,
  out: [number, number, number] = [0, 0, 0]
): [number, number, number] {
  out[0] = fbm3(x, y, z, seed, options)
  out[1] = fbm3(x, y, z, seed + 7919, options)
  out[2] = fbm3(x, y, z, seed + 15839, options)
  return out
}
