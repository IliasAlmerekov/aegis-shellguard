import { describe, expect, it } from 'vitest'
import {
  createSampler,
  initialTier,
  nextTierDown,
  resolveDpr,
  sampleFrame,
  TIER_ORDER,
} from '@/lib/scene/quality'
import { quality } from '@/lib/scene/config'

/** Feed `n` frames of a given duration through the sampler. */
function run(frames: number, seconds: number, from = createSampler()) {
  let state = from
  for (let i = 0; i < frames; i++) state = sampleFrame(state, seconds)
  return state
}

const FAST = 1 / 120
const SLOW = 1 / 20

describe('nextTierDown', () => {
  it('walks the ladder and stops at the bottom', () => {
    expect(nextTierDown('full')).toBe('reduced')
    expect(nextTierDown('reduced')).toBe('low')
    expect(nextTierDown('low')).toBe('still')
    expect(nextTierDown('still')).toBeNull()
  })
})

describe('sampleFrame', () => {
  it('holds the tier while frames are fast', () => {
    expect(run(quality.sampleWindow * 3, FAST).tier).toBe('full')
  })

  it('does not decide before the window is full', () => {
    const state = run(quality.sampleWindow - 1, SLOW)
    expect(state.tier).toBe('full')
    expect(state.samples).toHaveLength(quality.sampleWindow - 1)
  })

  it('drops exactly one tier per full window of slow frames', () => {
    let state = run(quality.sampleWindow, SLOW)
    expect(state.tier).toBe('reduced')

    state = run(quality.sampleWindow, SLOW, state)
    expect(state.tier).toBe('low')
  })

  it('empties the window on every decision', () => {
    // Otherwise the next tier is judged partly on the previous tier's frames
    // and the ladder skips a rung.
    const state = run(quality.sampleWindow, SLOW)
    expect(state.samples).toHaveLength(0)
  })

  it('never climbs back up', () => {
    let state = run(quality.sampleWindow, SLOW)
    expect(state.tier).toBe('reduced')
    state = run(quality.sampleWindow * 5, FAST, state)
    expect(state.tier).toBe('reduced')
  })

  it('stops at the bottom of the ladder', () => {
    let state = createSampler('still')
    state = run(quality.sampleWindow * 3, SLOW, state)
    expect(state.tier).toBe('still')
  })

  it('ignores non-positive deltas', () => {
    // A backgrounded tab reports these, and averaged in they read as
    // infinitely fast frames.
    let state = createSampler()
    state = sampleFrame(state, 0)
    state = sampleFrame(state, -1)
    expect(state.samples).toHaveLength(0)
  })
})

describe('resolveDpr', () => {
  it('caps the display ratio before applying the tier scale', () => {
    // A DPR-3 phone on the lowest tier must not slip past the cap by way of
    // the multiplication.
    expect(resolveDpr(3, 'full')).toBe(quality.maxDpr)
    expect(resolveDpr(3, 'low')).toBe(quality.maxDpr * 0.4)
  })

  it('never renders below one device pixel per CSS pixel of scale', () => {
    expect(resolveDpr(0.5, 'full')).toBe(1)
  })
})

describe('initialTier', () => {
  it('starts at the top, or at the still frame when motion is unwelcome', () => {
    expect(initialTier(false)).toBe('full')
    expect(initialTier(true)).toBe('still')
  })

  it('has a settings entry for every tier on the ladder', () => {
    for (const tier of TIER_ORDER) {
      expect(quality.tiers[tier]).toBeDefined()
    }
  })
})
