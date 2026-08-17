/**
 * Dropping a tier based on measured frame time.
 *
 * `quality.ts` describes the ladder but only picks the starting tier — from
 * viewport width and `saveData`. That is not enough: the spread of hardware
 * within one width is wider than the spread between widths, and a laptop on
 * integrated graphics gets the same settings as a machine with a discrete GPU.
 * A steady sixty frames comes from measuring, not from guessing at device
 * traits.
 *
 * Only the arithmetic of the window lives here, with no three and no React:
 * the rule "we fall and never climb back" is checked by a test, whereas by eye
 * it takes minutes to catch.
 */

import { LADDER, TIERS, type Tier } from './config'

/** The next tier down, or null when there is nowhere lower to go. */
export function nextTier(tier: Tier): Tier | null {
  const index = TIERS.indexOf(tier)
  if (index < 0 || index >= TIERS.length - 1) return null
  return TIERS[index + 1]
}

/** The ladder's numbers. Not `typeof LADDER`: those are literal types, and a
 *  test could not substitute a window of its own. */
export type LadderConfig = { [K in keyof typeof LADDER]: number }

export type Watcher = {
  /**
   * One frame of length `deltaMs`. True means the window closed above budget
   * and the tier should be dropped.
   */
  push(deltaMs: number): boolean
  /** Start a new window: called after a tier change. */
  reset(): void
}

/**
 * The measurement window.
 *
 * The median is computed when the window closes, not on every frame: sorting
 * sixty numbers sixty times a second is precisely the work a performance
 * monitor has no business doing.
 *
 * Warm-up, window and cooldown are each given as a **pair** of limits — in
 * frames and in milliseconds — and close on whichever arrives first. Frames
 * alone would stretch the ladder further the weaker the machine; time alone
 * would collect three times more frames than a median needs on a fast one.
 */
export function createWatcher(config: LadderConfig = LADDER): Watcher {
  const samples = new Float32Array(config.window)
  const sorted = new Float32Array(config.window)

  let filled = 0
  let elapsed = 0

  let skipFrames: number = config.warmup
  let skipMs: number = config.warmupMs

  return {
    push(deltaMs) {
      // A stall is not a frame. It says nothing about the machine but would
      // spoil the whole window, and the windows here are not long.
      if (!Number.isFinite(deltaMs) || deltaMs <= 0 || deltaMs > config.outlier) {
        return false
      }

      if (skipFrames > 0 && skipMs > 0) {
        skipFrames -= 1
        skipMs -= deltaMs
        return false
      }

      samples[filled] = deltaMs
      filled += 1
      elapsed += deltaMs

      const full = filled >= config.window
      const late = elapsed >= config.windowMs && filled >= config.minSamples
      if (!full && !late) return false

      sorted.fill(0, 0, config.window)
      sorted.set(samples.subarray(0, filled))
      sorted.subarray(0, filled).sort()
      const median = sorted[filled >> 1]

      filled = 0
      elapsed = 0
      if (median <= config.budget) return false

      // Cooldown after a drop: the new tier rebuilds geometry and buffers, and
      // its first frames cost more than its settled ones. Measuring those
      // would slide the whole ladder in a second.
      skipFrames = config.cooldown
      skipMs = config.cooldownMs
      return true
    },

    reset() {
      filled = 0
      elapsed = 0
      skipFrames = config.cooldown
      skipMs = config.cooldownMs
    },
  }
}
