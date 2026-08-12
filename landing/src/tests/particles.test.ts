import { describe, expect, it } from 'vitest'
import { createParticleField, createRandom } from '@/lib/scene/particles'
import { particles } from '@/lib/scene/config'

describe('createRandom', () => {
  it('is deterministic for a seed', () => {
    const a = createRandom(1234)
    const b = createRandom(1234)
    expect([a(), a(), a()]).toEqual([b(), b(), b()])
  })

  it('produces different streams for different seeds', () => {
    expect(createRandom(1)()).not.toBe(createRandom(2)())
  })

  it('stays inside [0, 1)', () => {
    const r = createRandom(99)
    for (let i = 0; i < 2000; i++) {
      const v = r()
      expect(v).toBeGreaterThanOrEqual(0)
      expect(v).toBeLessThan(1)
    }
  })
})

describe('createParticleField', () => {
  const COUNT = 500
  const field = createParticleField(COUNT)

  it('fills every buffer to the requested count', () => {
    expect(field.count).toBe(COUNT)
    expect(field.positions).toHaveLength(COUNT * 3)
    expect(field.lit).toHaveLength(COUNT)
    expect(field.phase).toHaveLength(COUNT)
  })

  it('keeps every particle inside the shell', () => {
    // The camera flies through this cloud and the matter sits at the origin,
    // so a particle inside the inner radius would be inside the body.
    const inner = particles.shellRadius - particles.shellThickness
    for (let i = 0; i < COUNT; i++) {
      const r = Math.hypot(
        field.positions[i * 3],
        field.positions[i * 3 + 1],
        field.positions[i * 3 + 2]
      )
      expect(r).toBeGreaterThanOrEqual(inner - 1e-6)
      expect(r).toBeLessThanOrEqual(particles.shellRadius + 1e-6)
    }
  })

  it('has no preferred axis', () => {
    // Sampling latitude uniformly would pile particles at the poles, and the
    // pole points at the viewer. The mean of a well-spread shell is near the
    // origin on every axis.
    let sx = 0
    let sy = 0
    let sz = 0
    for (let i = 0; i < COUNT; i++) {
      sx += field.positions[i * 3]
      sy += field.positions[i * 3 + 1]
      sz += field.positions[i * 3 + 2]
    }
    const tolerance = particles.shellRadius * 0.15
    expect(Math.abs(sx / COUNT)).toBeLessThan(tolerance)
    expect(Math.abs(sy / COUNT)).toBeLessThan(tolerance)
    expect(Math.abs(sz / COUNT)).toBeLessThan(tolerance)
  })

  it('lights only a minority, and marks them with a flag not a colour', () => {
    const litCount = Array.from(field.lit).filter((v) => v === 1).length
    expect(litCount).toBeGreaterThan(0)
    expect(litCount / COUNT).toBeLessThan(0.2)
    for (const v of field.lit) expect([0, 1]).toContain(v)
  })

  it('is reproducible across calls', () => {
    const again = createParticleField(COUNT)
    expect(Array.from(again.positions)).toEqual(Array.from(field.positions))
  })
})
