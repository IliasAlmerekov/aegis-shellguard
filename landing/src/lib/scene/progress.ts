/**
 * Pin progress → scene state.
 *
 * No three.js and no GSAP here: only the mapping of one number onto several.
 * Mistakes in this arithmetic look like "the camera jumped" or "the beat got
 * skipped", and catching those by eye costs more than a test does.
 */

/** The stretch of the pin over which an event happens. */
export type Span = { readonly from: number; readonly to: number }

export function clamp01(value: number): number {
  return value < 0 ? 0 : value > 1 ? 1 : value
}

/** Progress through a span: 0 before it starts, 1 after it ends. */
export function spanProgress(progress: number, span: Span): number {
  if (span.to <= span.from) return progress >= span.to ? 1 : 0
  return clamp01((progress - span.from) / (span.to - span.from))
}

/** Smooth in and out: zero derivative at both ends. */
export function smoothstep(t: number): number {
  const x = clamp01(t)
  return x * x * (3 - 2 * x)
}

/**
 * Deceleration that is quick on entry and lazy on exit.
 *
 * Under a scrub the clock *is* the visitor, which makes exponential curves
 * unusable: `expo.out` spends nine tenths of its motion on the first tenth of
 * the scroll, so the event snaps shut within a few pixels and then crawls.
 * Quadratic is the limit past which this must not go.
 */
export function easeOut(t: number): number {
  const x = clamp01(t)
  return 1 - (1 - x) * (1 - x)
}

export type Accent = {
  /** Bounds of the stretch that is given the extra scroll. */
  readonly from: number
  /** Where the deceleration peaks. It need not be in the middle. */
  readonly peak: number
  readonly to: number
  /** How many times more scroll the stretch costs at its peak. */
  readonly gain: number
}

const DENSITY_STEPS = 512

/**
 * The scroll density map.
 *
 * What is declared is not a partition into segments but the **price of
 * scroll**: how many pixels each part of the scene costs. The density
 * `1 + gain · bump` is integrated once into a table and read back, turning pin
 * progress into scene progress.
 *
 * The bump is a raised cosine, and the shape matters more than the magnitude
 * here. Two or three linear segments meeting at `from` and `to` would be
 * continuous in *position* but not in *speed*: at each joint the speed changes
 * instantly, and the scene audibly shifts gear entering and leaving the
 * deceleration. A raised cosine has zero derivative at all three nodes, so the
 * deceleration arrives, peaks and departs at the common pace.
 *
 * The map is monotonic, so scrolling back behaves as playback in reverse and
 * breaks nothing.
 */
export function densityMap(accent: Accent): (progress: number) => number {
  const bump = (u: number): number => {
    if (u <= accent.from || u >= accent.to) return 0
    const half =
      u < accent.peak
        ? (u - accent.from) / (accent.peak - accent.from)
        : (accent.to - u) / (accent.to - accent.peak)
    return 0.5 * (1 - Math.cos(Math.PI * clamp01(half)))
  }

  const cumulative = new Float64Array(DENSITY_STEPS + 1)
  for (let step = 1; step <= DENSITY_STEPS; step += 1) {
    const u0 = (step - 1) / DENSITY_STEPS
    const u1 = step / DENSITY_STEPS
    const d0 = 1 + accent.gain * bump(u0)
    const d1 = 1 + accent.gain * bump(u1)
    cumulative[step] = cumulative[step - 1] + (d0 + d1) / 2
  }

  const total = cumulative[DENSITY_STEPS]
  for (let step = 0; step <= DENSITY_STEPS; step += 1) cumulative[step] /= total

  return (progress: number): number => {
    const clamped = clamp01(progress)
    let low = 0
    let high = DENSITY_STEPS
    while (high - low > 1) {
      const mid = (low + high) >> 1
      if (cumulative[mid] <= clamped) low = mid
      else high = mid
    }
    const width = cumulative[high] - cumulative[low]
    const t = width > 0 ? (clamped - cumulative[low]) / width : 0
    return (low + t) / DENSITY_STEPS
  }
}

export type Stage = {
  /** The copy departing: 1 means the copy is gone. */
  readonly copyOut: number
  /** The camera closing in, and the cube turning to face front. */
  readonly approach: number
  /** The fissure opening. */
  readonly opening: number
  /** The parts' answering move: the guardrail locks an opening already begun. */
  readonly lock: number
  /** Position of the light pulse travelling from centre to edges. */
  readonly pulse: number
  /** The camera's final run to the locked fissure. */
  readonly handoff: number
  /** Defocus and darkening on the way out. */
  readonly exit: number
}

export type Choreography = {
  readonly copy: Span
  readonly approach: Span
  readonly opening: Span
  readonly lock: Span
  readonly pulse: Span
  readonly handoff: Span
  readonly exit: Span
  readonly accent: Accent
}

/**
 * The scene's state at a given pin progress.
 *
 * Curves are assigned by role, not by taste. The camera approach is
 * `smoothstep`: zero derivative at both ends, so the camera starts moving and
 * stops without a jerk. The copy departure and the exit are `easeOut`: both
 * have to happen early within their span so they stop competing with the main
 * event.
 */
export function stageAt(progress: number, plan: Choreography, map: (p: number) => number): Stage {
  const scene = map(progress)
  return {
    copyOut: easeOut(spanProgress(scene, plan.copy)),
    approach: smoothstep(spanProgress(scene, plan.approach)),
    opening: smoothstep(spanProgress(scene, plan.opening)),
    lock: smoothstep(spanProgress(scene, plan.lock)),
    pulse: smoothstep(spanProgress(scene, plan.pulse)),
    handoff: smoothstep(spanProgress(scene, plan.handoff)),
    exit: easeOut(spanProgress(scene, plan.exit)),
  }
}
