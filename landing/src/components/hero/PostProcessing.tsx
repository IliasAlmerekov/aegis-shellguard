'use client'

import { useMemo, useRef, type RefObject } from 'react'
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
import { BlendFunction, ToneMappingMode } from 'postprocessing'
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
 * Defocus and aberration only exist over the last few percent of the pin, but
 * they live in the chain permanently: mounting a pass mid-flight would
 * recompile the composer's shader at precisely the moment the picture is meant
 * to drift. Their strength ramps from zero instead, and until the exit they
 * cost next to nothing.
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

  useFrame(() => {
    if (!settings.finalOptics) return
    const stage = stageAt(progress.current ?? 0, SCROLL, map)

    if (dof.current) dof.current.bokehScale = SCROLL.bokeh * stage.exit
    if (aberration.current) {
      const amount = SCROLL.aberration * stage.exit
      aberration.current.offset.set(amount, amount * 0.6)
    }
  })

  return (
    <EffectComposer multisampling={settings.multisampling}>
      {settings.bloom ? (
        <Bloom
          intensity={POST.bloom.intensity}
          luminanceThreshold={POST.bloom.threshold}
          luminanceSmoothing={POST.bloom.smoothing}
          radius={POST.bloom.radius}
          mipmapBlur
        />
      ) : (
        <></>
      )}
      {settings.finalOptics ? (
        // Defocus is computed at a fraction of the resolution, not at full: a
        // bokeh pass is a blur, and its own sharpness is by definition
        // invisible. The pass costs a fraction of the price that way, and it is
        // in the chain permanently — see above for why it cannot be mounted on
        // the fly.
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
      {settings.finalOptics ? (
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
