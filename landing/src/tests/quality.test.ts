import { describe, expect, it } from 'vitest'

import { DPR, TIER_SETTINGS } from '../lib/scene/config'
import { budgetedDpr, freezeFromSearch } from '../lib/scene/quality'

describe('freezeFromSearch', () => {
  it('ignores an empty freeze value', () => {
    expect(freezeFromSearch('?freeze=')).toBeNull()
  })
})

const full = TIER_SETTINGS.full

/** Drawn pixels, which is the quantity the budget is actually about. */
const drawn = (width: number, height: number, dpr: number) =>
  width * dpr * height * dpr

describe('budgetedDpr', () => {
  it('leaves a retina laptop on the tier ceiling', () => {
    // 14-inch MacBook Pro: 1.48 megapixels of viewport, density 2. The budget
    // would allow 1.53, so the ceiling is what binds, exactly as before.
    expect(
      budgetedDpr({ width: 1512, height: 982, devicePixelRatio: 2 }, full)
    ).toBe(1.5)
  })

  it('leaves a 1440p desktop at native resolution', () => {
    expect(
      budgetedDpr({ width: 2560, height: 1440, devicePixelRatio: 1 }, full)
    ).toBe(1)
  })

  it('never draws above the display density', () => {
    // A small window on a non-retina screen: both the ceiling and the budget
    // permit more than 1, and drawing more than the display can show is waste.
    expect(
      budgetedDpr({ width: 1280, height: 720, devicePixelRatio: 1 }, full)
    ).toBe(1)
  })

  it('cuts a 5K retina display the density ceiling cannot reach', () => {
    const viewport = { width: 2560, height: 1440, devicePixelRatio: 2 }
    const dpr = budgetedDpr(viewport, full)

    expect(dpr).toBeLessThan(full.maxDpr)
    expect(drawn(viewport.width, viewport.height, dpr)).toBeLessThanOrEqual(
      full.pixelBudget
    )
    // What the old range would have given: 8.3 megapixels against a 4.0 budget.
    expect(drawn(viewport.width, viewport.height, full.maxDpr)).toBeGreaterThan(
      2 * full.pixelBudget
    )
  })

  it('cuts a 4K panel whose reported density is one', () => {
    // The case a dpr ceiling is blind to: density 1, so no ceiling above 1 can
    // engage, yet the viewport alone is 8.3 megapixels.
    const viewport = { width: 3840, height: 2160, devicePixelRatio: 1 }
    const dpr = budgetedDpr(viewport, full)

    expect(dpr).toBeLessThan(1)
    expect(dpr).toBeGreaterThanOrEqual(DPR.floor)
  })

  it('stops descending at the floor', () => {
    // An absurd viewport: the budget alone would ask for a ratio low enough to
    // read as an upscaled image. The floor wins and the frame goes over budget.
    expect(
      budgetedDpr({ width: 7680, height: 4320, devicePixelRatio: 1 }, full)
    ).toBe(DPR.floor)
  })

  it('quantises to the step so a window drag cannot thrash the buffers', () => {
    const a = budgetedDpr({ width: 2600, height: 1440, devicePixelRatio: 2 }, full)
    const b = budgetedDpr({ width: 2604, height: 1440, devicePixelRatio: 2 }, full)

    expect(a).toBe(b)
    expect(Math.round(a / DPR.step) * DPR.step).toBeCloseTo(a, 10)
  })

  it('survives a viewport measured before the first layout', () => {
    expect(
      budgetedDpr({ width: 0, height: 0, devicePixelRatio: 2 }, full)
    ).toBe(full.maxDpr)
  })

  it('descends with the tier', () => {
    const viewport = { width: 1512, height: 982, devicePixelRatio: 2 }
    const ladder = [
      TIER_SETTINGS.full,
      TIER_SETTINGS.reduced,
      TIER_SETTINGS.low,
    ].map((settings) => budgetedDpr(viewport, settings))

    expect(ladder[0]).toBeGreaterThan(ladder[1])
    expect(ladder[1]).toBeGreaterThan(ladder[2])
  })
})
