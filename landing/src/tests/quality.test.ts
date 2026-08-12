import { describe, expect, it } from 'vitest'
import {
  createSampler,
  initialTier,
  nextTierDown,
  resolveDpr,
  sampleFrame,
  settingsFor,
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
  /* Asserted against `settingsFor`, not against the tiers' current numbers:
     render scales are artistic settings and are expected to move. What must
     not move is the rule — cap first, scale second. */

  it('caps the display ratio before applying the tier scale', () => {
    // The order matters: scaling first would let a DPR-3 phone on a 0.4 tier
    // arrive at 1.2 and slip under a cap it should have hit at 3.
    for (const tier of TIER_ORDER) {
      expect(resolveDpr(3, tier)).toBeCloseTo(
        quality.maxDpr * settingsFor(tier).renderScale
      )
    }
  })

  it('never renders a display at less than one device pixel per CSS pixel', () => {
    // Before the tier's own scale, that is: a sub-1 devicePixelRatio is
    // something a zoomed-out browser reports, not an instruction to render
    // the scene at a quarter of the page's resolution.
    const tier = 'full'
    expect(resolveDpr(0.5, tier)).toBeCloseTo(settingsFor(tier).renderScale)
  })

  it('gives every tier a scale at or below the one above it', () => {
    let previous = Infinity
    for (const tier of ['full', 'reduced', 'low'] as const) {
      const scale = settingsFor(tier).renderScale
      expect(scale).toBeLessThanOrEqual(previous)
      previous = scale
    }
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
