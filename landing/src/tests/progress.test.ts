import { describe, expect, it } from 'vitest'

import { SCROLL } from '../lib/scene/config'
import {
  clamp01,
  densityMap,
  easeOut,
  smoothstep,
  spanProgress,
  stageAt,
} from '../lib/scene/progress'

describe('spans', () => {
  const span = { from: 0.2, to: 0.6 }

  it('stay silent before the start and hold one after the end', () => {
    expect(spanProgress(0, span)).toBe(0)
    expect(spanProgress(0.2, span)).toBe(0)
    expect(spanProgress(0.6, span)).toBe(1)
    expect(spanProgress(1, span)).toBe(1)
  })

  it('run linearly inside', () => {
    expect(spanProgress(0.4, span)).toBeCloseTo(0.5, 10)
  })

  it('do not divide by zero on a degenerate span', () => {
    expect(spanProgress(0.3, { from: 0.5, to: 0.5 })).toBe(0)
    expect(spanProgress(0.7, { from: 0.5, to: 0.5 })).toBe(1)
  })
})

describe('curves', () => {
  it('are pinned at both ends', () => {
    for (const ease of [smoothstep, easeOut]) {
      expect(ease(0)).toBe(0)
      expect(ease(1)).toBe(1)
      expect(ease(-3)).toBe(0)
      expect(ease(4)).toBe(1)
    }
  })

  it('are monotonic', () => {
    for (const ease of [smoothstep, easeOut]) {
      let previous = -Infinity
      for (let i = 0; i <= 200; i += 1) {
        const value = ease(i / 200)
        expect(value).toBeGreaterThanOrEqual(previous)
        previous = value
      }
    }
  })

  it('smoothstep stands still at both ends, easeOut only at the far one', () => {
    const slope = (ease: (t: number) => number, at: number) =>
      (ease(at + 1e-4) - ease(at - 1e-4)) / 2e-4

    expect(Math.abs(slope(smoothstep, 0.0002))).toBeLessThan(0.01)
    expect(Math.abs(slope(smoothstep, 0.9998))).toBeLessThan(0.01)

    expect(slope(easeOut, 0.0002)).toBeGreaterThan(1.5)
    expect(Math.abs(slope(easeOut, 0.9998))).toBeLessThan(0.01)
  })
})

/**
 * The density map is the one place where a mistake does not show as a breakage
 * but reads as "the page is lagging". Three of its properties are checked: the
 * ends are pinned, it is monotonic (otherwise scrolling back would jerk the
 * scene forward), and it really does hand the accent span more scroll than its
 * share.
 */
describe('scroll density map', () => {
  const map = densityMap(SCROLL.accent)

  it('is pinned at both ends', () => {
    expect(map(0)).toBeCloseTo(0, 6)
    expect(map(1)).toBeCloseTo(1, 6)
  })

  it('is monotonic', () => {
    let previous = -Infinity
    for (let i = 0; i <= 1000; i += 1) {
      const value = map(i / 1000)
      expect(value).toBeGreaterThanOrEqual(previous - 1e-12)
      previous = value
    }
  })

  it('stays in range for an argument outside it', () => {
    expect(map(-1)).toBeCloseTo(0, 6)
    expect(map(2)).toBeCloseTo(1, 6)
  })

  it('hands the accent more scroll than its share of the scene', () => {
    const { from, to } = SCROLL.accent
    const share = to - from

    // How much of the pin it takes for the scene to cross the accent span.
    let entered = 1
    let left = 1
    for (let i = 0; i <= 2000; i += 1) {
      const pin = i / 2000
      const scene = map(pin)
      if (scene >= from && pin < entered) entered = pin
      if (scene >= to) {
        left = pin
        break
      }
    }

    expect(left - entered).toBeGreaterThan(share * 1.3)
  })

  it('collapses to the identity with no gain', () => {
    const flat = densityMap({ ...SCROLL.accent, gain: 0 })
    for (const p of [0.1, 0.35, 0.5, 0.77, 0.95]) {
      expect(flat(p)).toBeCloseTo(p, 3)
    }
  })
})

describe('scene state', () => {
  const map = densityMap(SCROLL.accent)
  const at = (p: number) => stageAt(p, SCROLL, map)

  it('has everything at rest at the start of the pin', () => {
    const stage = at(0)
    expect(stage.copyOut).toBe(0)
    expect(stage.approach).toBe(0)
    expect(stage.opening).toBe(0)
    expect(stage.lock).toBe(0)
    expect(stage.pulse).toBe(0)
    expect(stage.handoff).toBe(0)
    expect(stage.exit).toBe(0)
  })

  it('has everything played out at the end of the pin', () => {
    const stage = at(1)
    expect(stage.copyOut).toBe(1)
    expect(stage.approach).toBe(1)
    expect(stage.opening).toBe(1)
    expect(stage.lock).toBe(1)
    expect(stage.pulse).toBe(1)
    expect(stage.handoff).toBe(1)
    expect(stage.exit).toBe(1)
  })

  it('drives the copy by the same span as the camera approach', () => {
    // This is the whole content of the fly-past: the copy stands in the
    // stone's space, and it is the camera's motion that carries it off, not a
    // clock of its own. Different spans would mean two spaces that happen to
    // be overlaid.
    expect(SCROLL.copy).toBe(SCROLL.approach)

    for (const p of [0, 0.1, 0.25, 0.5, 0.75, 1]) {
      const stage = at(p)
      // The curves differ by role — the copy is `easeOut`, the camera
      // `smoothstep` — but they have to reach zero and one at the same points.
      expect(stage.copyOut === 0).toBe(stage.approach === 0)
      expect(stage.copyOut === 1).toBe(stage.approach === 1)
    }
  })

  it('gets the copy past before the stone starts opening', () => {
    // The opening is the scene's main event, and it must not be covered by the
    // copy.
    let openingStarted = 1
    for (let i = 0; i <= 1000; i += 1) {
      const p = i / 1000
      if (at(p).opening > 0) {
        openingStarted = p
        break
      }
    }
    expect(at(openingStarted).copyOut).toBeGreaterThan(0.98)
  })

  it('finishes the camera approach before the darkening starts', () => {
    let approachDone = 1
    let exitStarted = 1
    for (let i = 0; i <= 1000; i += 1) {
      const p = i / 1000
      const stage = at(p)
      if (stage.approach >= 0.999 && approachDone === 1) approachDone = p
      if (stage.exit > 0 && exitStarted === 1) exitStarted = p
    }
    expect(approachDone).toBeLessThan(exitStarted)
  })

  it('starts the Policy Lock only after the opening is nearly complete', () => {
    let lockStarted = 1
    for (let i = 0; i <= 1000; i += 1) {
      const p = i / 1000
      if (at(p).lock > 0) {
        lockStarted = p
        break
      }
    }

    expect(at(lockStarted).opening).toBeGreaterThan(0.9)
    // What is left after the impact is a narrow slit of light, not the former
    // near-full opening: otherwise the new gesture is visually
    // indistinguishable from the old scene.
    expect(SCROLL.lockRecoil).toBeGreaterThan(SCROLL.open * 0.7)
    expect(SCROLL.lockRecoil).toBeLessThan(SCROLL.open)
  })

  it('starts the handoff only after the lock’s main impact', () => {
    expect(SCROLL.handoff.from).toBeGreaterThan(SCROLL.lock.from)
    expect(SCROLL.handoff.from).toBeGreaterThanOrEqual(
      SCROLL.lock.from + (SCROLL.lock.to - SCROLL.lock.from) * 0.7
    )
  })

  it('keeps every phase monotonic in pin progress', () => {
    const keys = ['copyOut', 'approach', 'opening', 'lock', 'pulse', 'handoff', 'exit'] as const
    const previous: Record<string, number> = {
      copyOut: -1,
      approach: -1,
      opening: -1,
      lock: -1,
      pulse: -1,
      handoff: -1,
      exit: -1,
    }
    for (let i = 0; i <= 1000; i += 1) {
      const stage = at(i / 1000)
      for (const key of keys) {
        expect(stage[key]).toBeGreaterThanOrEqual(previous[key] - 1e-12)
        previous[key] = stage[key]
      }
    }
  })
})

describe('clamp01', () => {
  it('clamps to the bounds', () => {
    expect(clamp01(-5)).toBe(0)
    expect(clamp01(5)).toBe(1)
    expect(clamp01(0.42)).toBe(0.42)
  })
})
