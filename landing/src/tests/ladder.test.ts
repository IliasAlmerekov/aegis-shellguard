import { describe, expect, it } from 'vitest'

import { LADDER } from '../lib/scene/config'
import { createWatcher, nextTier, type LadderConfig } from '../lib/scene/ladder'

/** Run n frames of equal length, returning the number of downgrade signals. */
function run(watcher: ReturnType<typeof createWatcher>, frames: number, ms: number): number {
  let signals = 0
  for (let i = 0; i < frames; i += 1) if (watcher.push(ms)) signals += 1
  return signals
}

/**
 * How many frames of length `ms` pass before the first signal.
 *
 * This is the very quantity the two limits per stretch exist for: it has to
 * stay within sensible seconds at any speed, rather than growing along with
 * the weight of a frame.
 */
function framesToFirstSignal(ms: number, limit = 4000): number {
  const watcher = createWatcher()
  for (let frame = 1; frame <= limit; frame += 1) {
    if (watcher.push(ms)) return frame
  }
  return Number.POSITIVE_INFINITY
}

describe('nextTier', () => {
  it('walks the ladder strictly downward', () => {
    expect(nextTier('full')).toBe('reduced')
    expect(nextTier('reduced')).toBe('low')
    expect(nextTier('low')).toBe('still')
  })

  it('stops at the bottom tier', () => {
    expect(nextTier('still')).toBeNull()
  })
})

describe('createWatcher', () => {
  it('stays silent on in-budget frames, however many there are', () => {
    const watcher = createWatcher()
    expect(run(watcher, 2000, 12)).toBe(0)
  })

  it('drops a tier once frames go over budget', () => {
    expect(framesToFirstSignal(40)).toBeLessThan(Number.POSITIVE_INFINITY)
  })

  it('does not judge by warm-up frames: the first signal arrives well after the start', () => {
    // Shader compilation and map loading live in the first frames, and judged
    // by those every machine looks weak.
    expect(framesToFirstSignal(40)).toBeGreaterThan(LADDER.minSamples)
  })

  it('reaches a verdict in seconds rather than minutes, at any speed', () => {
    // Exactly what the time limit on each stretch exists for: without it a weak
    // machine would take longer to accumulate ninety warm-up frames the more
    // badly it needed the downgrade.
    // Only out-of-budget frames here: 17 ms is 58 fps, and there is nothing to
    // downgrade at that rate. The upper end is a software rasteriser — nothing
    // real is slower, and that is exactly where the old outlier threshold
    // stayed silent forever.
    for (const ms of [25, 40, 60, 90, 250, 833]) {
      const seconds = (framesToFirstSignal(ms) * ms) / 1000
      expect(seconds).toBeLessThan(12)
    }
  })

  it('drops a tier even on a machine where a frame takes nearly a second', () => {
    // Regression: at an outlier threshold of 100 ms such frames never entered
    // the window at all, the window never closed, and the weakest machine was
    // left on the heaviest tier.
    expect(framesToFirstSignal(833)).toBeLessThan(Number.POSITIVE_INFINITY)
  })

  it('does not slide down the whole ladder on one sag', () => {
    const watcher = createWatcher()
    let signals = 0
    // Five seconds of heavy frames in a row give several downgrades, but a
    // countable few — not one per frame.
    for (let i = 0; i < 125; i += 1) if (watcher.push(40)) signals += 1
    expect(signals).toBeGreaterThanOrEqual(1)
    expect(signals).toBeLessThanOrEqual(2)
  })

  it('judges by the median, not the mean: a single spike does not sink the window', () => {
    const watcher = createWatcher()
    // Warm up on fast frames.
    run(watcher, LADDER.warmup, 10)

    let signals = 0
    // One 90 ms frame among fast ones: the mean goes over budget, the median
    // does not.
    if (watcher.push(90)) signals += 1
    signals += run(watcher, LADDER.window * 4, 10)
    expect(signals).toBe(0)
  })

  it('does not count a hidden tab’s stall as a frame', () => {
    const watcher = createWatcher()
    // Outliers take no place in the window and do not move its limits, so the
    // window never closes.
    expect(run(watcher, 4000, LADDER.outlier + 1)).toBe(0)
  })

  it('starts with a pause after a reset, not with a verdict', () => {
    const watcher = createWatcher()
    run(watcher, 400, 40)
    watcher.reset()
    expect(run(watcher, LADDER.minSamples, 40)).toBe(0)
  })
})

describe('createWatcher: stretch limits', () => {
  const base: LadderConfig = { ...LADDER }

  it('closes a stretch on the frame limit when frames are fast', () => {
    // 60 frames of 5 ms is 300 ms, nowhere near `windowMs`, so the window has
    // to close on the frame count.
    const config: LadderConfig = { ...base, warmup: 0, warmupMs: 0, budget: 1 }
    const watcher = createWatcher(config)
    expect(run(watcher, config.window - 1, 5)).toBe(0)
    expect(watcher.push(5)).toBe(true)
  })

  it('closes a stretch on the time limit when frames are heavy', () => {
    // 90 ms per frame: `windowMs` accumulates in 17 frames, long before sixty.
    const config: LadderConfig = { ...base, warmup: 0, warmupMs: 0 }
    const watcher = createWatcher(config)
    let frames = 0
    while (!watcher.push(90)) frames += 1
    frames += 1
    expect(frames).toBeLessThan(config.window)
    expect(frames * 90).toBeGreaterThanOrEqual(config.windowMs)
  })

  it('does not let the time limit close the window before the median means anything', () => {
    // A 50 ms window would close on the very first frame, but `minSamples`
    // holds it open: the median of one number is that number.
    const config: LadderConfig = { ...base, warmup: 0, warmupMs: 0, windowMs: 50 }
    const watcher = createWatcher(config)
    expect(run(watcher, config.minSamples - 1, 40)).toBe(0)
    expect(watcher.push(40)).toBe(true)
  })
})
