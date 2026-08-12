'use client'

import { Bloom, DepthOfField, EffectComposer } from '@react-three/postprocessing'
import { post } from '@/lib/scene/config'

type Props = {
  bloom: boolean
  /** 0 → 1 across the tail of the scroll. Below 1e-3 the pass is dropped
      entirely rather than run at zero strength. */
  dof: number
}

export function PostProcessing({ bloom, dof }: Props) {
  const wantsDof = dof > 0.001

  /* Nothing to composite: mounting an EffectComposer with no passes still
     costs a full-screen copy every frame, and on the tiers where bloom is off
     that copy is exactly the budget the tier was trying to save. */
  if (!bloom && !wantsDof) return null

  return (
    <EffectComposer>
      {bloom ? (
        <Bloom
          /* Selective by threshold alone. Only the veins' emissive exceeds it,
             which is what keeps the bloom off the matter itself — the failure
             mode where the hero's first impression becomes "blue glow". */
          luminanceThreshold={post.bloomThreshold}
          luminanceSmoothing={post.bloomSmoothing}
          intensity={post.bloomIntensity}
          mipmapBlur
        />
      ) : (
        <></>
      )}
      {wantsDof ? (
        <DepthOfField
          focusDistance={post.dofFocusDistance}
          focalLength={post.dofFocalLength}
          /* Scaled by progress so the blur arrives rather than switches on. */
          bokehScale={post.dofBokehScale * dof}
        />
      ) : (
        <></>
      )}
    </EffectComposer>
  )
}
