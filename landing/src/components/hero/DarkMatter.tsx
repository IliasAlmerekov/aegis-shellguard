'use client'

import { useMemo, useRef } from 'react'
import { useFrame, useLoader } from '@react-three/fiber'
import { useTexture } from '@react-three/drei'
import { RGBELoader } from 'three/examples/jsm/loaders/RGBELoader.js'
import * as THREE from 'three'

import { assets, camera as cameraConfig, motion, pointer } from '@/lib/scene/config'
import { settingsFor } from '@/lib/scene/quality'
import type { QualityTier } from '@/lib/scene/config'
import { darkMatterFragment, darkMatterVertex } from '@/lib/shaders/darkMatter'

type Props = {
  tier: QualityTier
  /** False on the `still` tier, where one frame is drawn and time stops. */
  animate: boolean
}

export function DarkMatter({ tier, animate }: Props) {
  const materialRef = useRef<THREE.ShaderMaterial>(null)

  /* The pointer the shader sees is not the pointer the browser reports: it
     approaches it exponentially, so a flick of the mouse is something the
     mass catches up with rather than something it mirrors. */
  const damped = useRef(new THREE.Vector2())
  const spin = useRef(0)

  const env = useLoader(RGBELoader, assets.environment)
  const [normalMap, roughnessMap] = useTexture([
    assets.normalMap,
    assets.roughnessMap,
  ])

  const envLods = useMemo(() => {
    /* Equirectangular, sampled by direction, with roughness choosing a mip.
       Mipmaps have to be asked for explicitly — RGBELoader does not generate
       them, and without them every roughness would return the sharpest
       level and the surface would read as chrome. */
    env.mapping = THREE.EquirectangularReflectionMapping
    env.wrapS = THREE.RepeatWrapping
    env.wrapT = THREE.ClampToEdgeWrapping
    env.minFilter = THREE.LinearMipmapLinearFilter
    env.magFilter = THREE.LinearFilter
    env.generateMipmaps = true
    env.needsUpdate = true
    return Math.log2(Math.max(env.image?.width ?? 1024, 1))
  }, [env])

  useMemo(() => {
    for (const t of [normalMap, roughnessMap]) {
      t.wrapS = THREE.RepeatWrapping
      t.wrapT = THREE.RepeatWrapping
      /* The maps are tiled across a body whose surfaces face every direction,
         so plenty of them meet the camera at a grazing angle. Anisotropy is
         what keeps those from smearing into grey. */
      t.anisotropy = 8
      t.needsUpdate = true
    }
  }, [normalMap, roughnessMap])

  const uniforms = useMemo(
    () => ({
      uTime: { value: 0 },
      uResolution: { value: new THREE.Vector2(1, 1) },
      uCamPos: { value: new THREE.Vector3() },
      uCamRight: { value: new THREE.Vector3() },
      uCamUp: { value: new THREE.Vector3() },
      uCamForward: { value: new THREE.Vector3() },
      uTanHalfFov: { value: Math.tan((cameraConfig.fov * Math.PI) / 360) },
      uViewProjection: { value: new THREE.Matrix4() },
      uPointer: { value: new THREE.Vector2() },
      uBreath: { value: 1 },
      uSpin: { value: 0 },
      uMaxSteps: { value: settingsFor('full').maxSteps },
      uUseSss: { value: 1 },
      uEnv: { value: env },
      uEnvLods: { value: envLods },
      uNormalMap: { value: normalMap },
      uRoughnessMap: { value: roughnessMap },
    }),
    [env, envLods, normalMap, roughnessMap]
  )

  useFrame((state, delta) => {
    const u = uniforms
    const settings = settingsFor(tier)

    u.uMaxSteps.value = settings.maxSteps
    u.uUseSss.value = settings.sss ? 1 : 0

    const cam = state.camera as THREE.PerspectiveCamera
    u.uCamPos.value.copy(cam.position)
    u.uTanHalfFov.value = Math.tan((cam.fov * Math.PI) / 360)

    /* The basis is read off the camera's own matrix rather than rebuilt from
       a look-at: the camera is driven by scroll elsewhere, and deriving the
       basis here from anything but its final matrix would put the rays one
       frame behind the image. */
    cam.updateMatrixWorld()
    const e = cam.matrixWorld.elements
    u.uCamRight.value.set(e[0], e[1], e[2])
    u.uCamUp.value.set(e[4], e[5], e[6])
    u.uCamForward.value.set(-e[8], -e[9], -e[10])

    u.uViewProjection.value
      .copy(cam.projectionMatrix)
      .multiply(cam.matrixWorldInverse)

    const size = state.size
    u.uResolution.value.set(size.width, size.height)

    if (!animate) return

    u.uTime.value += delta

    /* Exponential approach, framed so the rate is per second and therefore
       independent of frame rate — the same flick settles in the same wall
       time on a 60 Hz laptop and a 144 Hz monitor. */
    const k = 1 - Math.exp(-pointer.damping * delta)
    damped.current.x += (state.pointer.x - damped.current.x) * k
    damped.current.y += (state.pointer.y - damped.current.y) * k
    u.uPointer.value.copy(damped.current)

    spin.current += motion.rotationSpeed * delta
    u.uSpin.value = spin.current

    u.uBreath.value =
      1 +
      Math.sin((u.uTime.value / motion.breathPeriod) * Math.PI * 2) *
        motion.breathAmplitude
  })

  return (
    /* Placed in clip space by the vertex shader, so it needs no transform and
       must never be culled by a frustum it does not live in. It draws first
       and writes depth by hand, which is what lets the particle cloud sort
       against a surface that has no geometry. */
    <mesh frustumCulled={false} renderOrder={-1}>
      <planeGeometry args={[1, 1]} />
      <shaderMaterial
        ref={materialRef}
        vertexShader={darkMatterVertex}
        fragmentShader={darkMatterFragment}
        uniforms={uniforms}
        glslVersion={THREE.GLSL3}
        /* Both on, and both load-bearing. The depth buffer is only written
           when the depth test is enabled — disabling the test to let a
           full-screen quad through would also throw away the hand-written
           gl_FragDepth, and the particle cloud would lose every cue about
           what is in front of it. The quad passes the test anyway: the
           buffer is cleared to 1.0 and every fragment writes something at or
           below that. */
        depthTest
        depthWrite
      />
    </mesh>
  )
}
