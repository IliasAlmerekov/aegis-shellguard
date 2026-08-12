/**
 * The quality ladder.
 *
 * A frame-rate sampler and a one-way descent. Pure and framework-free so the
 * policy — which is the part that can go wrong — is testable without a GPU.
 */

import { quality, type QualityTier } from './config'

export const TIER_ORDER: readonly QualityTier[] = [
  'full',
  'reduced',
  'low',
  'still',
]

export type Sampler = {
  readonly tier: QualityTier
  /** Frame durations in seconds, since the last decision. */
  readonly samples: readonly number[]
}

export function createSampler(tier: QualityTier = 'full'): Sampler {
  return { tier, samples: [] }
}

/** The tier one step below, or null at the bottom of the ladder. */
export function nextTierDown(tier: QualityTier): QualityTier | null {
  const i = TIER_ORDER.indexOf(tier)
  return i < 0 || i >= TIER_ORDER.length - 1 ? null : TIER_ORDER[i + 1]
}

/**
 * Feed one frame's duration in and get the sampler back.
 *
 * The window is only evaluated once it is full, and it is emptied on every
 * decision — including a decision to stay. Carrying samples across a tier
 * change would judge the new tier partly on the old tier's frames, which is
 * how a ladder skips a rung it never needed to.
 *
 * Zero and negative durations are dropped rather than averaged: a browser
 * that has been backgrounded reports them, and they would read as an
 * infinitely fast frame.
 */
export function sampleFrame(state: Sampler, deltaSeconds: number): Sampler {
  if (!(deltaSeconds > 0)) return state

  const samples = [...state.samples, deltaSeconds]
  if (samples.length < quality.sampleWindow) return { ...state, samples }

  const total = samples.reduce((a, b) => a + b, 0)
  const fps = samples.length / total

  if (fps >= quality.downgradeFps) return { ...state, samples: [] }

  const next = nextTierDown(state.tier)
  /* Already at the bottom: keep sampling but stop deciding. There is nothing
     left to give up, and a scene that has reached `still` costs almost
     nothing anyway. */
  if (next === null) return { ...state, samples: [] }

  return { tier: next, samples: [] }
}

/** The render settings for a tier. */
export function settingsFor(tier: QualityTier) {
  return quality.tiers[tier]
}

/**
 * Device pixel ratio to render at, given the display's own ratio and a tier.
 *
 * Capped before the tier's scale is applied, not after: the cap exists to
 * stop a high-DPR phone asking for pixels the scene cannot use, and applying
 * it afterwards would let DPR 3 × scale 0.4 slip past a cap of 2.
 */
export function resolveDpr(devicePixelRatio: number, tier: QualityTier): number {
  const capped = Math.min(Math.max(devicePixelRatio, 1), quality.maxDpr)
  return capped * settingsFor(tier).renderScale
}

/**
 * Where the ladder starts.
 *
 * `prefers-reduced-motion` goes straight to `still`: the tier renders one
 * frame of the real scene and stops, which is the honest reading of the
 * preference — the visitor asked for no motion, not for no image.
 */
export function initialTier(prefersReducedMotion: boolean): QualityTier {
  return prefersReducedMotion ? 'still' : 'full'
}
