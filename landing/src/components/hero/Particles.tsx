'use client'

import { useMemo, useRef, type RefObject } from 'react'
import { useFrame } from '@react-three/fiber'
import * as THREE from 'three'

import { particles } from '@/lib/scene/config'
import { createParticleField } from '@/lib/scene/particles'
import type { SceneState } from '@/lib/scene/progress'

type Props = {
  /** Per-frame scene state, read inside the render loop rather than taken as
      plain props: both values it needs change on every frame of the scroll. */
  sceneRef: RefObject<SceneState>
  animate: boolean
  /** Scaled down with the quality tier — the cloud is the cheapest thing in
      the scene to give up, and the last thing anyone would miss. */
  count: number
}

export function Particles({ sceneRef, animate, count }: Props) {
  const pointsRef = useRef<THREE.Points>(null)
  const materialRef = useRef<THREE.PointsMaterial>(null)
  const elapsed = useRef(0)

  const { geometry, base } = useMemo(() => {
    const field = createParticleField(count)
    const g = new THREE.BufferGeometry()
    g.setAttribute('position', new THREE.BufferAttribute(field.positions, 3))

    /* Colour is baked per vertex rather than branched in a shader: there are
       two colours in the whole cloud, and a vertex attribute costs nothing to
       read where a uniform branch would cost a material. */
    const colors = new Float32Array(count * 3)
    const dim = new THREE.Color(...particles.dimColor)
    const lit = new THREE.Color(...particles.litColor)
    for (let i = 0; i < count; i++) {
      const c = field.lit[i] === 1 ? lit : dim
      colors[i * 3] = c.r
      colors[i * 3 + 1] = c.g
      colors[i * 3 + 2] = c.b
    }
    g.setAttribute('color', new THREE.BufferAttribute(colors, 3))

    return { geometry: g, base: field }
  }, [count])

  useFrame((_, delta) => {
    const { insideCloud } = sceneRef.current

    if (materialRef.current) {
      /* The cloud fades up as the camera arrives rather than existing from
         the first frame: seen from outside it would read as dust on the lens,
         and the whole point of it is the moment of passing through. */
      materialRef.current.opacity = 0.25 + 0.75 * insideCloud
      materialRef.current.size = particles.size * (1 + insideCloud * 0.6)
    }

    if (!animate || !pointsRef.current) return

    elapsed.current += delta

    /* Drift, applied to the whole cloud as a slow rotation plus a per-particle
       bob. Moving every particle individually on the CPU would cost more than
       the cloud is worth; the rotation alone reads as suspension. */
    pointsRef.current.rotation.y += particles.drift * delta
    const attr = geometry.getAttribute('position') as THREE.BufferAttribute
    const arr = attr.array as Float32Array
    for (let i = 0; i < base.count; i++) {
      arr[i * 3 + 1] =
        base.positions[i * 3 + 1] +
        Math.sin(elapsed.current * 0.4 + base.phase[i]) * 0.03
    }
    attr.needsUpdate = true
  })

  return (
    <points ref={pointsRef} geometry={geometry}>
      <pointsMaterial
        ref={materialRef}
        vertexColors
        transparent
        // Additive on near-black would make the dim majority invisible and
        // the lit few into stars. Normal blending keeps them as matter.
        depthWrite={false}
        sizeAttenuation
        size={particles.size}
      />
    </points>
  )
}
