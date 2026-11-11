/**
 * Fitting the stone to the frame.
 *
 * The camera's field of view is vertical, so a phone frame keeps the stone's
 * height and loses its width: at 375 px the frame is narrower than the turned
 * cube. The stone is scaled down to the width the frame has, never up.
 */

import { BALANCE, CAMERA, CUBE } from './config'

export function stoneScale(aspect: number): number {
  const distance = Math.hypot(
    CAMERA.rest[0],
    CAMERA.rest[1] - CUBE.centerY,
    CAMERA.rest[2]
  )
  const frameWidth = 2 * distance * Math.tan((CAMERA.fov * Math.PI) / 360) * aspect

  // The cube stands turned about Y, so both its width and its depth show.
  const yaw = BALANCE.pose[1]
  const stoneWidth = CUBE.size * (Math.abs(Math.cos(yaw)) + Math.abs(Math.sin(yaw)))

  return Math.min(1, (frameWidth * CUBE.fit) / stoneWidth)
}

/**
 * How far the scaled stone is raised, in scene units.
 *
 * A stone shrunk about its centre leaves a gap under the navigation as wide
 * as the one it opens above the copy. The lift closes the upper gap, so the
 * room the phone frame has goes to the copy below.
 */
export function stoneLift(aspect: number): number {
  return (1 - stoneScale(aspect)) * CUBE.narrowLift
}
