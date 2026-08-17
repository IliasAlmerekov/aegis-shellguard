'use client'

import { useMemo, type RefObject } from 'react'
import { useFrame } from '@react-three/fiber'
import { Vector3 } from 'three'

import { CAMERA, CUBE, SCROLL } from '../../lib/scene/config'
import { densityMap, stageAt } from '../../lib/scene/progress'

type Props = {
  /**
   * Pin progress, 0..1. A ref rather than a prop value: scroll arrives per
   * frame, and React state would re-render the whole scene every frame for the
   * sake of a single number.
   */
  progress: RefObject<number>
}

/**
 * The scroll-driven camera.
 *
 * The camera is the one object in the scene with *two* motion parameters:
 * where it stands and where it looks. Both are driven by the same phase, so
 * the approach cannot drift out of step with the aim.
 *
 * At rest the aim sits at the origin while the cube hangs above it — that is
 * where the composition with the stone high in the frame comes from. By the
 * end of the approach the aim has risen to the cube's centre, and the stone is
 * centred and fills the frame. It is the same gesture as "the camera moved
 * closer", only stated honestly: closing in without moving the aim would push
 * the stone off the top edge.
 */
export function CameraRig({ progress }: Props) {
  const map = useMemo(() => densityMap(SCROLL.accent), [])

  const restEye = useMemo(() => new Vector3(...CAMERA.rest), [])
  const restAim = useMemo(() => new Vector3(...CAMERA.target), [])
  const closeEye = useMemo(
    () => new Vector3(...SCROLL.close).add(new Vector3(0, CUBE.centerY, 0)),
    []
  )
  const closeAim = useMemo(() => new Vector3(0, CUBE.centerY, 0), [])
  const handoffEye = useMemo(
    () => new Vector3(...SCROLL.handoffEye).add(new Vector3(0, CUBE.centerY, 0)),
    []
  )
  const handoffAim = useMemo(
    () => new Vector3(...SCROLL.handoffAim).add(new Vector3(0, CUBE.centerY, 0)),
    []
  )

  const eye = useMemo(() => new Vector3(), [])
  const aim = useMemo(() => new Vector3(), [])

  useFrame(({ camera }) => {
    const stage = stageAt(progress.current ?? 0, SCROLL, map)

    eye.copy(restEye).lerp(closeEye, stage.approach)
    aim.copy(restAim).lerp(closeAim, stage.approach)

    // The lock has mass: at the point of contact the camera takes a short
    // shove backwards. The impulse returns to zero at both ends, so it rewinds
    // honestly when the visitor scrolls back up.
    const lockImpact = Math.sin(Math.PI * stage.lock)
    eye.z += SCROLL.lockCameraKick * lockImpact

    // Only after the impact has read does the camera slide toward the
    // now-locked fissure, turning it into a continuous transition to a black
    // frame.
    eye.lerp(handoffEye, stage.handoff)
    aim.lerp(handoffAim, stage.handoff)

    camera.position.copy(eye)
    camera.lookAt(aim)
  })

  return null
}
