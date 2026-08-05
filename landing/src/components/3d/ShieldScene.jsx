import { useRef, useEffect, Suspense } from 'react'
import { Canvas } from '@react-three/fiber'
import { useGSAP } from '@gsap/react'
import gsap from 'gsap'
import { Shield } from './Shield'

gsap.registerPlugin(useGSAP)

// Four green point lights placed around the shield. Because the shield yaws,
// each facet sweeps through several light cones in sequence → neon glints
// appear at different spots during the motion.
//
// The pulse amplitudes are deliberately small (≈15% of base): larger swings
// read as flicker rather than as light. Each light has its own period so the
// highlights never breathe in unison.
const LIGHTS = [
  // [x, y, z, baseIntensity, pulseAmp, period(s), color]
  [ 4,  5,  4, 22, 3, 5.0, '#c8f9b6' ],  // top-right key
  [-4,  4,  4, 14, 2, 6.4, '#7fee64' ],  // top-left
  [ 4, -4,  4, 12, 2, 7.1, '#7fee64' ],  // bottom-right fill
  [ 0,  0, -4,  8, 1, 8.3, '#7fee64' ],  // rim (behind)
]

function AnimatedLights({ active }) {
  const lightRefs  = useRef([])
  const timelineRef = useRef(null)

  useGSAP(() => {
    const lights = lightRefs.current.filter(Boolean)
    if (lights.length === 0) return

    const tl = gsap.timeline()
    timelineRef.current = tl

    lights.forEach((light, i) => {
      const [, , , base, amp, period] = LIGHTS[i]
      // Ramp up from dark so the shield lights up with the entrance instead of
      // appearing fully lit on frame one.
      tl.fromTo(light, { intensity: 0 }, { intensity: base, duration: 1.4, ease: 'power2.out' }, i * 0.12)
      tl.fromTo(light,
        { intensity: base - amp },
        { intensity: base + amp, duration: period / 2, ease: 'sine.inOut', repeat: -1, yoyo: true },
      1.6 + i * 0.35)
    })

    return () => { timelineRef.current = null }
  }, [])

  useEffect(() => {
    timelineRef.current?.paused(!active)
  }, [active])

  return (
    <>
      {LIGHTS.map(([x, y, z, base, , , color], i) => (
        <pointLight
          key={i}
          ref={(l) => { lightRefs.current[i] = l }}
          position={[x, y, z]}
          intensity={base}
          color={color}
        />
      ))}
    </>
  )
}

export function ShieldScene({ active = true }) {
  return (
    <Canvas
      camera={{ position: [0, 0, 5], fov: 52 }}
      gl={{ antialias: true, alpha: true }}
      style={{ background: 'transparent' }}
      dpr={[1, 2]}
      frameloop={active ? 'always' : 'never'}
    >
      <ambientLight intensity={0.05} />
      {/* Muted overhead cone — illuminates the top facets of the shield */}
      <spotLight
        position={[0, 9, 2]}
        angle={0.45}
        penumbra={0.9}
        intensity={18}
        color="#3d6b38"
        decay={2}
      />
      <AnimatedLights active={active} />
      <Suspense fallback={null}>
        <Shield active={active} />
      </Suspense>
    </Canvas>
  )
}
