'use client'

import { useMemo, useRef, useState, type RefObject } from 'react'
import { useFrame } from '@react-three/fiber'
import {
  Bloom,
  ChromaticAberration,
  DepthOfField,
  EffectComposer,
  Noise,
  ToneMapping,
  Vignette,
} from '@react-three/postprocessing'
import { BlendFunction, ToneMappingMode, type BloomEffect } from 'postprocessing'
import { Vector2 } from 'three'

import { POST, SCROLL, TIER_SETTINGS, type Tier } from '../../lib/scene/config'
import { densityMap, stageAt } from '../../lib/scene/progress'

type Props = {
  tier: Tier
  progress: RefObject<number>
}

/**
 * Pass order matters here, and none of it is arbitrary.
 *
 * Bloom works on luminance, so it has to run *before* tone mapping: after ACES
 * the core is already compressed into the displayable range and there is
 * nothing left to bloom. Vignette and grain come after, because they correct
 * the frame rather than the lighting.
 *
 * Tone mapping is moved here out of the renderer: the composer draws into a
 * linear buffer, and if the renderer applied ACES it would be applied twice.
 *
 * The composer itself is never dropped, on any tier. ACES lives only in this
 * chain, and the renderer is deliberately set to `NoToneMapping` — so a tier
 * that returned `null` here would ship an untone-mapped frame: the same scene,
 * visibly brighter with blown highlights, on exactly the weakest machine that
 * has no way to tell it is being shown something different from everyone else.
 * The lowest tier drops Bloom instead, which is the expensive pass; what stays
 * is one fullscreen pass that the frame needs to be the right frame at all.
 *
 * Defocus and aberration only exist over the last few percent of the pin, and
 * they used to live in the chain permanently on the grounds that mounting a
 * pass mid-flight recompiles the composer's shader at precisely the moment the
 * picture is meant to drift. That reasoning holds. What did not hold is the
 * claim that until the exit they cost next to nothing, and reading the library
 * says so plainly: `DepthOfFieldEffect.update` has no early-out, so at
 * `bokehScale` zero it still issues all seven of its renders, and
 * `ChromaticAberrationEffect` carries the convolution attribute, which earns
 * it a fullscreen pass of its own rather than a share of a merged one.
 *
 * Both therefore cost full price on the resting hero, which is where the
 * visitor spends the longest and, worse, where the quality ladder takes the
 * measurement that decides the tier. So they are armed at the first two
 * percent of the pin instead: early enough that the recompile lands before the
 * camera has started to move, once per visit rather than once per crossing.
 * See `SCROLL.opticsArm`.
 *
 * `multisampling` is set explicitly, and that matters more than anything else
 * in this file. `EffectComposer` defaults to eight, over a half-precision
 * buffer — eight bytes per pixel per sample. On a 3840×2160 frame that is
 * roughly half a gigabyte written and as much read back at resolve, every
 * frame: several milliseconds of pure memory traffic before a single fragment
 * of the scene has been shaded. What needs antialiasing here is the cube's
 * silhouette and the chipped edges — long, smooth boundaries where 4× and 8×
 * do not resolve apart under frame-by-frame comparison.
 */
export function PostProcessing({ tier, progress }: Props) {
  const settings = TIER_SETTINGS[tier]
  const map = useMemo(() => densityMap(SCROLL.accent), [])

  const dof = useRef<{ bokehScale: number } | null>(null)
  const aberration = useRef<{ offset: Vector2 } | null>(null)
  const bloom = useRef<BloomEffect | null>(null)

  /**
   * One-way, and it has to be: releasing the passes on the way back up would
   * turn one recompile per visit into one per crossing of the threshold, and
   * the threshold sits where a visitor scrolling back to read the headline
   * crosses it.
   */
  const [armed, setArmed] = useState(false)

  /**
   * Whether the bloom's luminance pass has been shrunk yet.
   *
   * The shrink is applied on the first frame rather than in a mount effect,
   * and that is not a matter of taste. `EffectComposer` mounts its children
   * after its own construction, so a mount effect in this component runs while
   * the ref is still null and the assignment is silently lost. Verified by
   * measurement: with the effect version the pass kept drawing at full size.
   */
  const luminanceTuned = useRef(false)

  useFrame(() => {
    /**
     * The luminance pass is shrunk through the ref rather than a prop because
     * the effect has no prop for it: `BloomEffect` builds its `LuminancePass`
     * with no resolution options, and its own `resolutionScale` is deprecated
     * and, under `mipmapBlur`, feeds a render target that is never sampled.
     * Assigning `resolution.scale` dispatches the library's own change event,
     * which resizes the pass's target properly, and the scale survives every
     * later resize because the effect's `setSize` only updates the base size.
     */
    if (!luminanceTuned.current && bloom.current) {
      bloom.current.luminancePass.resolution.scale = POST.bloom.luminanceScale
      luminanceTuned.current = true
    }

    if (!settings.finalOptics) return

    if (!armed && (progress.current ?? 0) > SCROLL.opticsArm) setArmed(true)

    const stage = stageAt(progress.current ?? 0, SCROLL, map)

    if (dof.current) dof.current.bokehScale = SCROLL.bokeh * stage.exit
    if (aberration.current) {
      const amount = SCROLL.aberration * stage.exit
      aberration.current.offset.set(amount, amount * 0.6)
    }
  })

  const optics = settings.finalOptics && armed

  return (
    <EffectComposer multisampling={settings.multisampling}>
      {settings.bloom ? (
        <Bloom
          ref={bloom as never}
          intensity={POST.bloom.intensity}
          luminanceThreshold={POST.bloom.threshold}
          luminanceSmoothing={POST.bloom.smoothing}
          radius={POST.bloom.radius}
          mipmapBlur
        />
      ) : (
        <></>
      )}
      {optics ? (
        // Defocus is computed at a fraction of the resolution: a bokeh pass is
        // a blur, and its own sharpness is by definition invisible. The saving
        // is smaller than the knob's name implies, though, because the effect
        // applies the scale to three of its nine render targets and leaves the
        // circle-of-confusion, blur and mask passes at full size. The rest of
        // the saving comes from `armed` above.
        <DepthOfField
          ref={dof as never}
          focusDistance={0}
          focalLength={0.02}
          bokehScale={0}
          resolutionScale={settings.dofResolution}
        />
      ) : (
        <></>
      )}
      {optics ? (
        <ChromaticAberration ref={aberration as never} offset={new Vector2(0, 0)} />
      ) : (
        <></>
      )}
      <ToneMapping mode={ToneMappingMode.ACES_FILMIC} />
      <Vignette
        offset={POST.vignette.offset}
        darkness={POST.vignette.darkness}
        eskil={false}
      />
      <Noise opacity={POST.grain.opacity} blendFunction={BlendFunction.OVERLAY} />
    </EffectComposer>
  )
}
