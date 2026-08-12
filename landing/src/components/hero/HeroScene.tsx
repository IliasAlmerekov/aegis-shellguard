'use client'

import { Suspense, useCallback, useRef, useState, type RefObject } from 'react'
import { Canvas, useFrame, useThree } from '@react-three/fiber'
import * as THREE from 'three'

import {
  camera as cameraConfig,
  particles as particlesConfig,
  type QualityTier,
} from '@/lib/scene/config'
import { cameraState, type SceneState } from '@/lib/scene/progress'
import {
  createSampler,
  initialTier,
  resolveDpr,
  sampleFrame,
  settingsFor,
  type Sampler,
} from '@/lib/scene/quality'
import { DarkMatter } from './DarkMatter'
import { Particles } from './Particles'
import { PostProcessing } from './PostProcessing'

type Props = {
  /** Written by the scroll driver every frame, read here. A ref rather than
      state on purpose: scroll produces a value 60 times a second, and
      re-rendering React at that rate to move a camera is pure waste. */
  progressRef: RefObject<number>
  prefersReducedMotion: boolean
}

/**
 * Everything that has to happen once per frame inside the canvas: the camera
 * move, the derived scene state, and the frame-rate sampling that drives the
 * quality ladder.
 */
function Rig({
  progressRef,
  sceneRef,
  tier,
  onTierChange,
  onDofChange,
}: {
  progressRef: RefObject<number>
  sceneRef: RefObject<SceneState>
  tier: QualityTier
  onTierChange: (tier: QualityTier) => void
  onDofChange: (quantised: number) => void
}) {
  const { camera } = useThree()
  const sampler = useRef<Sampler>(createSampler(tier))
  const target = useRef(new THREE.Vector3(...cameraConfig.target))
  const lastDof = useRef(0)

  useFrame((_, delta) => {
    const progress = progressRef.current ?? 0
    const next = cameraState(progress)

    camera.position.set(...next.position)
    camera.lookAt(target.current)

    const scene = sceneRef.current
    scene.progress = progress
    scene.opacity = next.opacity
    scene.insideCloud = next.insideCloud
    scene.dof = next.dof

    /* Depth of field is the one derived value that has to reach React, since
       mounting and sizing a pass is not something a ref can do. Quantising it
       to twentieths turns a per-frame value into at most twenty renders
       across the whole tail of the scroll. */
    const quantised = Math.round(next.dof * 20) / 20
    if (quantised !== lastDof.current) {
      lastDof.current = quantised
      onDofChange(quantised)
    }

    const sampled = sampleFrame(sampler.current, delta)
    if (sampled.tier !== sampler.current.tier) onTierChange(sampled.tier)
    sampler.current = sampled
  })

  return null
}

/** The scene's contents. Both children read the per-frame state out of the
    ref inside their own render loop, so nothing here re-renders during a
    scroll — only a tier change reaches React. */
function Contents({
  sceneRef,
  tier,
  animate,
}: {
  sceneRef: RefObject<SceneState>
  tier: QualityTier
  animate: boolean
}) {
  const scale = settingsFor(tier).renderScale
  /* Floored well above the render scale: at 0.4 the cloud would thin out at
     exactly the moment the camera flies into it, which is the one moment it
     exists for. */
  const count = Math.round(particlesConfig.count * Math.max(scale, 0.6))

  return (
    <>
      <DarkMatter tier={tier} animate={animate} />
      <Particles count={count} sceneRef={sceneRef} animate={animate} />
    </>
  )
}

export function HeroScene({ progressRef, prefersReducedMotion }: Props) {
  const [tier, setTier] = useState<QualityTier>(() =>
    initialTier(prefersReducedMotion)
  )
  const [dof, setDof] = useState(0)

  const sceneRef = useRef<SceneState>({
    progress: 0,
    opacity: 1,
    insideCloud: 0,
    dof: 0,
  })

  const dpr = useCallback(
    () => resolveDpr(typeof window === 'undefined' ? 1 : window.devicePixelRatio, tier),
    [tier]
  )

  const animate = tier !== 'still'
  const settings = settingsFor(tier)

  return (
    <Canvas
      /* On the still tier the loop is driven by demand: the scroll driver
         invalidates when the camera needs to move, and nothing else does. A
         frozen scene rendering sixty identical frames a second would defeat
         the entire point of the tier. */
      frameloop={animate ? 'always' : 'demand'}
      dpr={dpr()}
      gl={{
        antialias: false, // The image has no hard edges to alias; the marcher
        // resolves the silhouette itself, and MSAA on a full-screen quad
        // costs memory bandwidth for nothing.
        alpha: false,
        powerPreference: 'high-performance',
      }}
      camera={{
        fov: cameraConfig.fov,
        near: cameraConfig.near,
        far: cameraConfig.far,
        position: [...cameraConfig.startPosition],
      }}
    >
      <Suspense fallback={null}>
        <Rig
          progressRef={progressRef}
          sceneRef={sceneRef}
          tier={tier}
          onTierChange={setTier}
          onDofChange={setDof}
        />
        <Contents sceneRef={sceneRef} tier={tier} animate={animate} />
        <PostProcessing bloom={settings.bloom} dof={dof} />
      </Suspense>
    </Canvas>
  )
}
