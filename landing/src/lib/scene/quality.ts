/**
 * Quality tiers.
 *
 * The ladder is one-way: down only, never back up. A two-way ladder
 * oscillates — drop to `low`, frames get cheaper, climb back to `full`, sag
 * again — and the visitor watches the picture breathe quality at them. That is
 * worse than a steady `low`.
 *
 * The frame-time ladder itself lives in `ladder.ts`; what is here is what is
 * needed from the very first frame: the starting tier, and the forced tier
 * from the query string, without which a capture is not reproducible.
 */

import { DPR, TIERS, type Tier } from './config'

export function isTier(value: string | null | undefined): value is Tier {
  return typeof value === 'string' && (TIERS as readonly string[]).includes(value)
}

/**
 * Forced tier from the query string: `?tier=low`.
 *
 * It exists for the capture scripts: two frames are comparable only if they
 * were shot on the same tier, and on a SwiftShader machine the automatic
 * ladder slides down on the very first measurement window.
 */
export function tierFromSearch(search: string): Tier | null {
  const value = new URLSearchParams(search).get('tier')
  return isTier(value) ? value : null
}

/** Frozen scene time from the query string: `?freeze=12.5`. */
export function freezeFromSearch(search: string): number | null {
  const raw = new URLSearchParams(search).get('freeze')
  /* An empty value is rejected separately from a missing one, because
     `Number('')` is zero and zero here means "freeze at second zero".
     `?freeze=` is a typo in the address bar, not a request to stop the scene
     at its very beginning. */
  if (raw === null || raw.trim() === '') return null
  const value = Number(raw)
  return Number.isFinite(value) ? value : null
}

export type Environment = {
  /** `(max-width: 767px)` — the phone band. */
  narrow: boolean
  /** `navigator.connection.saveData`. */
  saveData: boolean
  /** `prefers-reduced-motion: reduce`. */
  reducedMotion: boolean
}

/**
 * The starting tier.
 *
 * A phone starts at `reduced` rather than `full`: the spread of hardware is
 * too wide to settle by measurement in front of the visitor — those first
 * frames get seen either way.
 *
 * `prefers-reduced-motion` is not about performance, so it does not lower the
 * tier: the scene stays at full quality and simply stops moving. Lowering it
 * would mean punishing someone for an accessibility setting.
 */
export function initialTier(environment: Environment): Tier {
  if (environment.saveData) return 'low'
  if (environment.narrow) return 'reduced'
  return 'full'
}

export type Viewport = {
  /** CSS pixels, not device pixels. */
  width: number
  height: number
  devicePixelRatio: number
}

export type DprLimits = {
  maxDpr: number
  pixelBudget: number
}

/**
 * The ratio the canvas is drawn at.
 *
 * Three ceilings, and the lowest of them wins. The display's own density,
 * because drawing above it buys nothing that can be shown. The tier's
 * `maxDpr`, which is the quality ladder's way down. And the tier's pixel
 * budget, which is the only one of the three that knows how large the viewport
 * is.
 *
 * The budget converts to a ratio by area: a ratio scales both axes, so the
 * count of pixels goes as its square, and the ratio that spends exactly the
 * budget is `sqrt(budget / area)`.
 *
 * The result is floored to `DPR.step` rather than rounded. Rounding could
 * cross a ceiling that was just applied, and a ceiling that the rounding is
 * allowed to exceed is not a ceiling.
 *
 * `DPR.floor` is applied last and deliberately overrides the budget: on a
 * viewport large enough to demand less, the frame is over budget on purpose.
 * The alternative is a picture soft enough to read as a scaled image, and a
 * scene that is visibly upscaled has not been made faster, it has been
 * replaced with a cheaper one.
 */
export function budgetedDpr(viewport: Viewport, limits: DprLimits): number {
  const area = viewport.width * viewport.height
  const byDensity = Math.min(viewport.devicePixelRatio, limits.maxDpr)

  // A viewport of zero area happens between mount and the first layout. There
  // is no ratio to derive from it, and dividing by it would give Infinity.
  const capped =
    area > 0 ? Math.min(byDensity, Math.sqrt(limits.pixelBudget / area)) : byDensity

  /* The epsilon is not caution but a requirement. `1.5 / 0.05` evaluates to
     29.999999999999996 in binary floating point, so a bare `floor` would
     answer 1.45 for a ratio that sits exactly on a step, and a retina laptop
     would quietly lose the resolution it was entitled to. The final rounding
     is for the same reason, from the other side: `29 * 0.05` is
     1.4500000000000002, and a ratio is compared for equality in the tests and
     used as a React key into a resize. */
  const steps = Math.floor(capped / DPR.step + 1e-9)
  const stepped = Math.round(steps * DPR.step * 1000) / 1000
  return Math.max(DPR.floor, stepped)
}
