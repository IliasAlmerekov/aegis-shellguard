import { useRef, useMemo, useEffect } from 'react'
import { useGSAP } from '@gsap/react'
import { Sparkles, useGLTF } from '@react-three/drei'
import gsap from 'gsap'
import * as THREE from 'three'

gsap.registerPlugin(useGSAP)

// Kick off GLB fetch before the Canvas mounts (no loader waterfall).
useGLTF.preload('/models/shield.glb')

// GLB bounds: X±0.807, Y±0.999, Z±0.110. Identity node transform, centered at origin.
// Scale 1.75 → ~3.5 units tall, filling ~72% of the 52-fov viewport at z=5.
const SCALE = 1.75
const HALF_H = 0.999 * SCALE   // 1.748 — vertical silhouette limit
const HALF_W = 0.807 * SCALE   // 1.412 — widest point of the silhouette

// The shield is a near-flat plate (Z is only ±0.11), so a full 360° spin turns
// it edge-on twice per revolution and reads as a glitch. Yaw oscillates inside
// a range that always keeps the face toward the camera instead.
const YAW = 0.3

// ─── GLB shield body ─────────────────────────────────────────────────────────
function GLBBody({ materialRef }) {
  const { scene } = useGLTF('/models/shield.glb')

  const cloned = useMemo(() => {
    const c = scene.clone(true)
    const mat = new THREE.MeshPhongMaterial({
      color:     '#07110a',
      emissive:  '#0b1c0b',
      specular:  '#7fee64',
      shininess: 130,    // sharp highlights = crisp neon glints on rotation
      transparent: true, // required for the entrance fade
      opacity:   0,
    })
    c.traverse((child) => {
      if (child.isMesh) {
        child.material   = mat
        child.castShadow = false
        child.receiveShadow = false
      }
    })
    return { object: c, material: mat }
  }, [scene])

  // Hand the material up so the parent's single timeline owns every tween.
  materialRef.current = cloned.material

  useEffect(() => {
    const mat = cloned.material
    return () => mat.dispose()
  }, [cloned])

  return <primitive object={cloned.object} scale={SCALE} />
}

// ─── Root export ──────────────────────────────────────────────────────────────
export function Shield({ active = true }) {
  const groupRef   = useRef(null)   // yaw / roll
  const bobRef     = useRef(null)   // vertical float (separate node: no tween conflict)
  const bodyMatRef = useRef(null)
  const auraMatRef = useRef(null)
  const ringMatRefs = useRef([])
  const scanRef    = useRef(null)
  const scanMatRef = useRef(null)
  const timelineRef = useRef(null)

  useGSAP(() => {
    const group = groupRef.current
    const bob   = bobRef.current
    const scan  = scanRef.current
    const rings = ringMatRefs.current.filter(Boolean)
    if (!group || !bob || !scan || !bodyMatRef.current) return

    const master = gsap.timeline()
    timelineRef.current = master

    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    // The scan line is deliberately excluded — its own sweep timeline owns its
    // opacity end to end, so fading it in here would fight that tween.
    const overlays = [auraMatRef.current, ...rings].filter(Boolean)
    // Remember each overlay's authored opacity so the fade-in lands on it.
    const targets = overlays.map((m) => ({ mat: m, to: m.opacity }))
    gsap.set(overlays, { opacity: 0 })
    gsap.set(scanMatRef.current, { opacity: 0 })
    // Vector3/Euler must be targeted directly — GSAP has no three.js shorthands.
    gsap.set(group.scale, { x: reduce ? 1 : 0.9, y: reduce ? 1 : 0.9, z: reduce ? 1 : 0.9 })
    gsap.set(group.rotation, { y: reduce ? -YAW : -YAW * 2.5, x: 0 })
    gsap.set(bob.position, { y: -0.055 })
    gsap.set(scan.position, { y: HALF_H })

    // ── Entrance: settle into the rest pose, then hand over to the idle loops.
    // Under reduced motion only the opacity fades — no travel, no scale.
    if (!reduce) {
      master
        .to(group.scale, { x: 1, y: 1, z: 1, duration: 1.3, ease: 'power3.out' }, 0)
        .to(group.rotation, { y: -YAW, x: -0.035, duration: 1.6, ease: 'power2.out' }, 0)
    }
    master.to(bodyMatRef.current, { opacity: 1, duration: 0.9, ease: 'power1.out' }, 0.1)

    targets.forEach(({ mat, to }, i) => {
      master.to(mat, { opacity: to, duration: 1.1, ease: 'power1.out' }, 0.45 + i * 0.08)
    })

    // Starts after the entrance tweens finish so nothing fights over the same
    // property (two live tweens on rotation.y would visibly jitter).
    const idle = gsap.timeline()
    master.add(idle, 2.0)

    const mm = gsap.matchMedia()

    mm.add('(prefers-reduced-motion: no-preference)', () => {
      // Every loop is sine.inOut + yoyo with a distinct period, so the motion
      // never snaps at a wrap point and the phases stay out of sync.
      idle.to(group.rotation, {
        y: YAW, duration: 6.5, ease: 'sine.inOut', repeat: -1, yoyo: true,
      }, 0)
      idle.fromTo(group.rotation,
        { x: -0.035 },
        { x: 0.035, duration: 5.2, ease: 'sine.inOut', repeat: -1, yoyo: true },
      0)
      idle.fromTo(bob.position,
        { y: -0.055 },
        { y: 0.055, duration: 3.4, ease: 'sine.inOut', repeat: -1, yoyo: true },
      0)

      if (auraMatRef.current) {
        idle.to(auraMatRef.current, {
          opacity: 0.085, duration: 4.6, ease: 'sine.inOut', repeat: -1, yoyo: true,
        }, 0)
      }

      rings.forEach((mat, i) => {
        idle.to(mat, {
          opacity: mat.opacity * 1.55,
          duration: 3.8 + i * 0.9,
          ease: 'sine.inOut',
          repeat: -1,
          yoyo: true,
        }, i * 0.6)
      })

      // Scan sweep: linear travel top → bottom, opacity fading at both ends so
      // it never pops, plus scaleX tracking the tapering silhouette. A pause
      // between passes keeps it from reading as strobing.
      const sweep = gsap.timeline({ repeat: -1, repeatDelay: 1.5 })
      sweep
        .fromTo(scan.position, { y: HALF_H * 0.94 }, { y: -HALF_H * 0.94, duration: 3.2, ease: 'none' }, 0)
        .fromTo(scan.scale, { x: 0.98 }, { x: 0.2, duration: 3.2, ease: 'power2.in' }, 0)
        .fromTo(scanMatRef.current, { opacity: 0 }, { opacity: 0.26, duration: 0.55, ease: 'power1.out' }, 0)
        .to(scanMatRef.current, { opacity: 0, duration: 0.7, ease: 'power1.in' }, 2.5)
      idle.add(sweep, 0)

      return () => {
        sweep.kill()
      }
    })

    return () => {
      mm.revert()
      timelineRef.current = null
    }
  }, [])

  // Off-screen the Canvas stops rendering (frameloop="never"); pause the
  // timeline in step so no CPU is spent tweening invisible objects.
  useEffect(() => {
    timelineRef.current?.paused(!active)
  }, [active])

  return (
    <group ref={bobRef}>
      <group ref={groupRef}>
        {/* Soft radial aura behind the model */}
        <mesh position={[0, 0, -0.25]}>
          <planeGeometry args={[4.8, 5.2]} />
          <meshBasicMaterial
            ref={auraMatRef}
            color="#1a4020"
            transparent
            opacity={0.055}
            blending={THREE.AdditiveBlending}
            depthWrite={false}
          />
        </mesh>

        <GLBBody materialRef={bodyMatRef} />

        {/* Two concentric corona rings — enough for the motif without noise */}
        {[
          { radius: 1.78, opacity: 0.09 },
          { radius: 1.5,  opacity: 0.17 },
        ].map((ring, i) => (
          <mesh key={ring.radius} position={[0, 0, 0.26]}>
            <ringGeometry args={[ring.radius - 0.006, ring.radius, 96]} />
            <meshBasicMaterial
              ref={(m) => { ringMatRefs.current[i] = m }}
              color="#7fee64"
              transparent
              opacity={ring.opacity}
              blending={THREE.AdditiveBlending}
              depthWrite={false}
              side={THREE.DoubleSide}
            />
          </mesh>
        ))}

        {/* Scan line sweeping top → bottom */}
        <mesh ref={scanRef} position={[0, HALF_H, 0.25]}>
          <planeGeometry args={[HALF_W * 2 * 0.95, 0.006]} />
          <meshBasicMaterial
            ref={scanMatRef}
            color="#7fee64"
            transparent
            opacity={0.26}
            blending={THREE.AdditiveBlending}
            depthWrite={false}
          />
        </mesh>

        <Sparkles
          count={28}
          scale={[3.4, 4.2, 1.6]}
          size={1}
          speed={0.12}
          color="#7fee64"
          opacity={0.35}
        />
      </group>
    </group>
  )
}
