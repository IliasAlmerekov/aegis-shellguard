'use client'

import { useThree } from '@react-three/fiber'
import { useLayoutEffect } from 'react'

import { CAMERA } from '../../lib/scene/config'

/**
 * The camera stands still: the scroll gathers the stone and moves nothing
 * else. The position comes from the canvas; only the aim is set here.
 */
export function CameraRig() {
  const camera = useThree((state) => state.camera)

  useLayoutEffect(() => {
    camera.lookAt(...CAMERA.target)
  }, [camera])

  return null
}
