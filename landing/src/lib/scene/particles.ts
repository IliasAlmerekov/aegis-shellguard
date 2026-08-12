/**
 * The particle cloud.
 *
 * Deterministic from a seed, so the arrangement can be tuned by eye and stay
 * tuned — and so a test can assert it without describing a picture.
 */

import { particles } from './config'

/**
 * Mulberry32. A 32-bit PRNG in four operations, chosen over
 * `Math.random` for the determinism and over anything larger because the
 * distribution only has to survive being looked at, not being analysed.
 */
export function createRandom(seed: number): () => number {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

export type ParticleField = {
  /** xyz triples. */
  positions: Float32Array
  /** One per particle: 0 for the near-black majority, 1 for the lit few. */
  lit: Float32Array
  /** Per-particle phase so drift does not move them in lockstep. */
  phase: Float32Array
  count: number
}

/**
 * Positions on a shell, not in a ball.
 *
 * The camera flies through this cloud, and the body sits at the origin —
 * particles near the centre would be inside the matter, where they would
 * either be hidden or, worse, poke through a fold. A shell also puts every
 * particle at a similar distance from the camera at the moment of the
 * pass-through, so the whole cloud reads as one event.
 *
 * Directions come from a normalised Gaussian-ish sum rather than from
 * spherical angles: sampling latitude uniformly clusters particles at the
 * poles, and the pole would be pointing straight at the viewer.
 */
export function createParticleField(
  count: number = particles.count
): ParticleField {
  const random = createRandom(particles.seed)
  const positions = new Float32Array(count * 3)
  const lit = new Float32Array(count)
  const phase = new Float32Array(count)

  const inner = particles.shellRadius - particles.shellThickness

  for (let i = 0; i < count; i++) {
    /* Three uniforms summed and centred approximate a normal distribution;
       normalising the resulting vector gives a direction with no preferred
       axis. */
    let x = random() + random() + random() - 1.5
    let y = random() + random() + random() - 1.5
    let z = random() + random() + random() - 1.5

    const len = Math.hypot(x, y, z) || 1
    x /= len
    y /= len
    z /= len

    /* Cube root of the uniform spreads particles evenly through the shell's
       volume. Interpolating the radius linearly would crowd them against the
       inner face, where the shell has less room. */
    const t = Math.cbrt(random())
    const radius = inner + (particles.shellRadius - inner) * t

    positions[i * 3] = x * radius
    positions[i * 3 + 1] = y * radius
    positions[i * 3 + 2] = z * radius

    lit[i] = random() < particles.litFraction ? 1 : 0
    phase[i] = random() * Math.PI * 2
  }

  return { positions, lit, phase, count }
}
