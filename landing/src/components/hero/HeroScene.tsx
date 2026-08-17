'use client'

import { Suspense, useCallback, useEffect, useRef, useState, type RefObject } from 'react'
import { Canvas, useFrame } from '@react-three/fiber'
import { Environment } from '@react-three/drei'
import { NoToneMapping } from 'three'

import {
  CAMERA,
  LIGHT,
  STILL_TIME,
  TIER_SETTINGS,
  type Tier,
} from '../../lib/scene/config'
import { createWatcher, nextTier } from '../../lib/scene/ladder'
import {
  freezeFromSearch,
  initialTier,
  tierFromSearch,
} from '../../lib/scene/quality'
import { CameraRig } from './CameraRig'
import { PostProcessing } from './PostProcessing'
import { StoneCube } from './StoneCube'

/**
 * A compact HDRI gives the PBR material a continuous reflected environment.
 *
 * The Studio Small 08 source is reduced to 512×256: at the stone's roughness
 * the fine detail disappears anyway, and the hero should not pay 1.5 MB for
 * sharpness nobody can see. The directional sources below keep the light
 * addressable and preserve the terminator.
 */
function Studio() {
  return <Environment files="/hdri/studio-small-08-512.hdr" background={false} />
}

/**
 * The frame-time meter.
 *
 * It lives as a component of its own and has to run after everything else:
 * `useFrame` without a priority runs in mount order, and the meter needs to be
 * last — otherwise it measures the interval up to its own call rather than the
 * length of the frame. R3F has no negative priority, so it simply stands last
 * in the markup, and that ordering is the only thing that places it.
 *
 * A component rather than a hook inside `HeroScene`: `useFrame` requires the
 * canvas context, and `HeroScene` draws the canvas itself and sits outside it.
 */
function QualityLadder({ tier, onDowngrade }: { tier: Tier; onDowngrade: (next: Tier) => void }) {
  const watcher = useRef(createWatcher())

  useEffect(() => {
    watcher.current.reset()
  }, [tier])

  useFrame((_, delta) => {
    if (!watcher.current.push(delta * 1000)) return
    const next = nextTier(tier)
    if (next) onDowngrade(next)
  })

  return null
}

type Props = {
  className?: string
  /** Pin progress, 0..1, by which scroll drives the whole scene. */
  progress: RefObject<number>
  /** Called once the first frame with every map has actually been drawn. */
  onReady?: () => void
}

/**
 * `Suspense` ends after the maps are decoded but before WebGL's first draw.
 * Signalling on the next rAF guarantees the loading layer only leaves after a
 * frame in which the browser could already have shown the finished stone.
 */
function FirstFrame({ onReady }: { onReady?: () => void }) {
  const reported = useRef(false)

  useFrame(() => {
    if (!onReady || reported.current) return
    reported.current = true
    requestAnimationFrame(onReady)
  })

  return null
}

export function HeroScene({ className, progress, onReady }: Props) {
  const [tier, setTier] = useState<Tier>('full')
  const [freeze, setFreeze] = useState<number | undefined>(undefined)
  const [interactive, setInteractive] = useState(false)
  const [reducedMotion, setReducedMotion] = useState(false)
  const [visible, setVisible] = useState(true)

  /**
   * A tier assigned by the query string is not moved by the ladder: a capture
   * is comparable only if two frames were shot on the same tier, and on a
   * SwiftShader machine the ladder slides down on the very first measurement
   * window.
   */
  const [pinned, setPinned] = useState(false)

  // The query is read after mount: a static export serves the same markup to
  // everyone, and there is nothing on a server to decide by the query string
  // with.
  useEffect(() => {
    const search = window.location.search
    const forced = tierFromSearch(search)
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    setPinned(forced !== null)
    setTier(
      forced ??
        initialTier({
          narrow: window.matchMedia('(max-width: 767px)').matches,
          saveData:
            (navigator as Navigator & { connection?: { saveData?: boolean } })
              .connection?.saveData === true,
          reducedMotion: reduced,
        })
    )
    setFreeze(freezeFromSearch(search) ?? undefined)
    setReducedMotion(reduced)

    // The cursor response exists only where there is a cursor. On a touch
    // device, telling a tap from a swipe costs around 200 ms, and a reaction
    // 200 ms after a light lift is lag, not response.
    setInteractive(window.matchMedia('(hover: hover)').matches && !reduced)
  }, [])

  const downgrade = useCallback(
    (next: Tier) => {
      if (pinned) return
      setTier(next)
    },
    [pinned]
  )

  /**
   * The canvas stops drawing as soon as the hero leaves the screen.
   *
   * Without this the scene keeps computing sixty frames a second behind the
   * back of the rest of the page: the sections below animate on scroll and
   * share both the main thread and the GPU with it. A 200vh pin guarantees
   * that the visitor spends most of the visit exactly where the scene is no
   * longer visible — that is, it stands switched off for longer than it stands
   * switched on.
   *
   * The 20% height margin is there so the canvas resumes before its edge
   * appears from under the fold: switching on costs one frame, and that frame
   * would land precisely on the visible edge.
   *
   * The observer attaches to the canvas through a ref rather than through
   * state captured in `onCreated`.
   *
   * The difference is not cosmetic. `onCreated` is called from inside the
   * canvas's mount, and `setState` from there forces React to re-render the
   * whole scene subtree at that very moment. In development StrictMode mounts
   * the tree twice with a teardown in between, and such a re-render lands on
   * an already-released context: `EffectComposer` asks it for attributes and
   * gets null. A ref gives the same element without re-rendering anything.
   */
  const canvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const observer = new IntersectionObserver(
      ([entry]) => setVisible(entry.isIntersecting),
      { rootMargin: '20% 0px' }
    )
    observer.observe(canvas)
    return () => observer.disconnect()
  }, [])

  const settings = TIER_SETTINGS[tier]

  /**
   * Under `prefers-reduced-motion` the scene renders on demand rather than on
   * a clock.
   *
   * With that setting the pin in `Hero.tsx` is never created, so there is
   * nowhere for progress to change from, and scene time is frozen — there is
   * no point drawing a second identical frame. This is at once what the
   * accessibility setting asks for and the cheapest mode the scene has.
   */
  const frameloop = reducedMotion ? 'demand' : visible ? 'always' : 'never'

  return (
    <Canvas
      ref={canvasRef}
      className={className}
      frameloop={frameloop}
      dpr={[1, settings.maxDpr]}
      gl={{
        // A deliberate lie. Antialiasing is the composer's job — the scene is
        // drawn into its buffer, not into the default one, and MSAA ordered
        // here would go to a canvas nobody draws into. That is not free
        // insurance but a second multisampled buffer of the same size in
        // memory.
        antialias: false,
        // The field the stone hangs in is drawn in the markup — the canvas has
        // to be transparent, or it would paint over it with its own black.
        alpha: true,
        // Neither the scene nor the composer needs a stencil, and without one
        // the depth buffer fits in 24 bits instead of 32.
        stencil: false,
        // On laptops with two GPUs the default choice is the integrated one.
        powerPreference: 'high-performance',
        // ACES lives in the composer, not here: bloom has to work on linear
        // luminance, before the compression into the displayable range.
        // Leaving it here would mean applying tone mapping twice.
        toneMapping: NoToneMapping,
        toneMappingExposure: LIGHT.exposure,
      }}
      camera={{
        fov: CAMERA.fov,
        position: [...CAMERA.rest],
        near: 0.1,
        far: 40,
      }}
      // `?probe=1` exposes the scene's state for the capture and inspection
      // scripts. Composition cannot be tuned by pixels on a screenshot: the
      // cube's size and position in frame follow from the fov, the distance
      // and the rise, and those are what has to be measured — not eyeballed.
      onCreated={(state) => {
        if (window.location.search.includes('probe')) {
          ;(window as unknown as { __aegisScene?: unknown }).__aegisScene = state
        }
      }}
    >
      <Suspense fallback={null}>
        <Studio />
        {/* The terminator. The environment gives soft ambient light, on which
            dark rock reads as a silhouette; the hard boundary of light and
            shadow creeping across the cavities is the only thing that makes
            the relief visible. */}
        <directionalLight
          intensity={LIGHT.sun.intensity}
          position={[...LIGHT.sun.position]}
          color={LIGHT.sun.color}
        />
        <directionalLight
          intensity={LIGHT.fill.intensity}
          position={[...LIGHT.fill.position]}
          color={LIGHT.fill.color}
        />
        <directionalLight
          intensity={LIGHT.top.intensity}
          position={[...LIGHT.top.position]}
          color={LIGHT.top.color}
        />
        <CameraRig progress={progress} />
        <StoneCube
          tier={tier}
          freeze={freeze ?? (reducedMotion ? STILL_TIME : undefined)}
          interactive={interactive}
          progress={progress}
        />
        <PostProcessing tier={tier} progress={progress} />
        {reducedMotion ? null : <QualityLadder tier={tier} onDowngrade={downgrade} />}
        <FirstFrame onReady={onReady} />
      </Suspense>
    </Canvas>
  )
}
