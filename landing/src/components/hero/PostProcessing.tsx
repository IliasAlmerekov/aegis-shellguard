'use client'

import { useRef } from 'react'
import { useFrame } from '@react-three/fiber'
import {
  Bloom,
  EffectComposer,
  Noise,
  ToneMapping,
  Vignette,
} from '@react-three/postprocessing'
import { BlendFunction, ToneMappingMode, type BloomEffect } from 'postprocessing'

import { POST, TIER_SETTINGS, type Tier } from '../../lib/scene/config'

type Props = {
  tier: Tier
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
 * `multisampling` is set explicitly, and that matters more than anything else
 * in this file. `EffectComposer` defaults to eight, over a half-precision
 * buffer — eight bytes per pixel per sample. On a 3840×2160 frame that is
 * roughly half a gigabyte written and as much read back at resolve, every
 * frame: several milliseconds of pure memory traffic before a single fragment
 * of the scene has been shaded. What needs antialiasing here is the cube's
 * silhouette and the chipped edges — long, smooth boundaries where 4× and 8×
 * do not resolve apart under frame-by-frame comparison.
 */
export function PostProcessing({ tier }: Props) {
  const settings = TIER_SETTINGS[tier]

  const bloom = useRef<BloomEffect | null>(null)

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
  })

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
