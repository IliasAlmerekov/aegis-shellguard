/**
 * Scroll progress and everything derived from it.
 *
 * Pure by design: the hero's whole scroll behaviour is decided here, where it
 * can be tested without a browser, a canvas or a GPU. The components only
 * feed this a number and apply what comes back.
 */

import { camera, scroll } from './config'

/** Clamp to [0, 1]. */
export function saturate(v: number): number {
  return v < 0 ? 0 : v > 1 ? 1 : v
}

/** Linear interpolation. */
export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t
}

/**
 * Remap `v` from [inMin, inMax] to [0, 1], clamped.
 *
 * Returns 0 rather than dividing by zero when the input range is empty — a
 * degenerate range means "this stage has no length", and no length means no
 * progress through it.
 */
export function remap(v: number, inMin: number, inMax: number): number {
  if (inMax === inMin) return 0
  return saturate((v - inMin) / (inMax - inMin))
}

/** Smoothstep on an already-normalised value. Used for anything a viewer
    watches start and stop — a linear fade betrays its own endpoints. */
export function smoothstep(t: number): number {
  const x = saturate(t)
  return x * x * (3 - 2 * x)
}

/**
 * Scroll position within the pinned hero, as 0 → 1.
 *
 * `scrolled` is how far past the top of the hero the page has moved and
 * `viewportHeight` is one screen. The pin is `scroll.screens` screens long,
 * so the denominator is the distance the visitor must travel to finish it.
 */
export function heroProgress(scrolled: number, viewportHeight: number): number {
  const span = viewportHeight * scroll.screens
  if (span <= 0) return 0
  return saturate(scrolled / span)
}

/**
 * The per-frame state of the scene, held in a ref and mutated in place.
 *
 * Deliberately mutable and deliberately outside React: these values change
 * every frame, and routing them through state would re-render the tree sixty
 * times a second to move numbers that only the render loop ever reads.
 */
export type SceneState = {
  progress: number
  opacity: number
  insideCloud: number
  dof: number
}

export type CameraState = {
  position: [number, number, number]
  /** 0 before the cloud, 1 once the camera is inside it. Drives particle
      opacity and size so they do not pop into existence. */
  insideCloud: number
  /** 0 → 1 across the depth-of-field tail. */
  dof: number
  /** 1 while the hero owns the screen, 0 once it has dissolved. */
  opacity: number
}

/**
 * The whole camera move for a given progress.
 *
 * The path is a straight line between two points, eased. A curve was tried
 * and rejected: with the camera always looking at the body, a curved approach
 * swings the mass across frame twice and reads as a mistake rather than as a
 * move.
 */
export function cameraState(progress: number): CameraState {
  const t = smoothstep(progress)
  const from = camera.startPosition
  const to = camera.endPosition

  return {
    position: [
      lerp(from[0], to[0], t),
      lerp(from[1], to[1], t),
      lerp(from[2], to[2], t),
    ],
    insideCloud: smoothstep(remap(progress, scroll.particleEntry, 1)),
    dof: smoothstep(remap(progress, scroll.dofStart, 1)),
    opacity: 1 - smoothstep(remap(progress, scroll.dofStart, scroll.fadeEnd)),
  }
}
