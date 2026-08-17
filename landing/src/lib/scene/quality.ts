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

import { TIERS, type Tier } from './config'

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
