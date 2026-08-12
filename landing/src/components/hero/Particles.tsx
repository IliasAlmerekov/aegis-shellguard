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
  const sprite = useSprite()

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
      materialRef.current.opacity = particles.opacity * (0.35 + 0.65 * insideCloud)
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
        map={sprite}
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

/**
 * A soft round dot.
 *
 * Without a map, a point is a hard square — which at the sizes this cloud
 * reaches during the fly-through reads as a screen full of confetti rather
 * than as suspended matter. Generated rather than shipped: it is a radial
 * gradient, and a 64px PNG of one would be an asset to load, cache and
 * account for in CREDITS for no gain.
 */
function useSprite(): THREE.Texture | null {
  return useMemo(() => {
    if (typeof document === 'undefined') return null

    const size = 64
    const canvas = document.createElement('canvas')
    canvas.width = size
    canvas.height = size

    const ctx = canvas.getContext('2d')
    if (!ctx) return null

    const g = ctx.createRadialGradient(
      size / 2,
      size / 2,
      0,
      size / 2,
      size / 2,
      size / 2
    )
    // Falls off well before the edge: a gradient that reaches the rim still
    // shows the quad's corners once hundreds of them overlap.
    g.addColorStop(0, 'rgba(255,255,255,1)')
    g.addColorStop(0.45, 'rgba(255,255,255,0.35)')
    g.addColorStop(1, 'rgba(255,255,255,0)')

    ctx.fillStyle = g
    ctx.fillRect(0, 0, size, size)

    const texture = new THREE.CanvasTexture(canvas)
    texture.needsUpdate = true
    return texture
  }, [])
}
