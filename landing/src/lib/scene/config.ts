/**
 * Every number the hero scene runs on.
 *
 * The rule from PLAN.md: a component may not carry a magic number. If a value
 * changes how the scene looks or feels, it lives here, and it carries a
 * comment saying why it is this value — not what it does, which the name
 * already says. "Too high" and "too low" are written down because the useful
 * range of most of these is narrow and invisible from the call site.
 */

/* ── Palette ─────────────────────────────────────────────────────────────
   The same eleven values as globals.css, as linear-ish floats the shader can
   use directly. Kept as tuples rather than THREE.Color so this module stays
   free of three and can be unit-tested in a plain Node process. */

export type Rgb = readonly [number, number, number]

const hex = (h: number): Rgb => [
  ((h >> 16) & 255) / 255,
  ((h >> 8) & 255) / 255,
  (h & 255) / 255,
]

export const palette = {
  nightVoid: hex(0x04090e),
  nightDeep: hex(0x141c22),
  night: hex(0x1e272e),
  nightEdge: hex(0x27333c),
  nightRim: hex(0x36424d),
  cloudFaint: hex(0x7d8894),
  cloudMute: hex(0x93a1ae),
  cloudDim: hex(0xaeb8c4),
  electric: hex(0x0984e3),
  electricLift: hex(0x3a9fea),
  cyanNeon: hex(0x00cec9),
} as const

/* ── The body ────────────────────────────────────────────────────────────
   A small core with tapered arms thrown out of it, welded by a smooth
   minimum. The arms are the point: a union of spheres, however deformed,
   returns a lump, and the reference is a splash — long thin ribbons
   radiating from a dense middle. That silhouette has to be built, not
   noised into existence. */

export const body = {
  /** Radius of the core. Small relative to the arms on purpose: the core is
      the dense middle they emerge from, not the subject. Raise it much past
      0.9 and the arms sink into it and the splash becomes a ball again. */
  coreRadius: 0.72,

  /**
   * The arms, as tapered capsules from `from` to `to` with a radius at each
   * end. `toRadius` well under `fromRadius` is what makes them ribbons that
   * thin out rather than sausages.
   *
   * Directions, lengths and thicknesses are all deliberately unequal and
   * deliberately not opposed in pairs. Any regularity here survives every
   * later stage and reads instantly as an ornament.
   */
  arms: [
    { from: [0.1, 0.3, 0.0], to: [0.35, 1.75, -0.25], fromRadius: 0.34, toRadius: 0.05 },
    { from: [-0.2, 0.1, 0.1], to: [-1.5, 0.85, 0.3], fromRadius: 0.3, toRadius: 0.04 },
    { from: [0.2, -0.1, 0.1], to: [1.62, 0.25, 0.15], fromRadius: 0.32, toRadius: 0.05 },
    { from: [-0.1, -0.2, 0.0], to: [-0.75, -1.35, -0.2], fromRadius: 0.28, toRadius: 0.04 },
    { from: [0.15, -0.25, 0.05], to: [0.9, -1.55, 0.35], fromRadius: 0.3, toRadius: 0.05 },
    { from: [0.0, 0.15, 0.2], to: [-0.45, 1.2, 0.95], fromRadius: 0.24, toRadius: 0.03 },
    { from: [0.1, 0.0, -0.15], to: [1.1, -0.7, -0.85], fromRadius: 0.26, toRadius: 0.04 },
    { from: [-0.15, 0.05, -0.1], to: [-1.15, -0.35, -0.7], fromRadius: 0.22, toRadius: 0.03 },
  ] as ReadonlyArray<{
    from: readonly [number, number, number]
    to: readonly [number, number, number]
    fromRadius: number
    toRadius: number
  }>,

  /**
   * The `smin` blend radius where arms meet the core and each other.
   *
   * Too low (< 0.1) and the arms read as tubes glued on. Too high (> 0.35)
   * and the webbing between them fills in until the silhouette closes back
   * into a blob — which is the failure this whole structure exists to avoid.
   */
  smoothness: 0.18,
} as const

/* ── Deformation ─────────────────────────────────────────────────────────
   The distance field is offset by fractal noise, which is what turns the
   welded spheres into folded matter. Three scales, weighted so the largest
   dominates: PLAN.md's rule that strong small-scale noise reads as rock or
   virus rather than as liquid. */

export const noise = {
  /** Spatial frequency of the large scale — the one that shapes the
      silhouette. Above ~1.2 the mass stops having a readable overall form and
      becomes evenly lumpy; below ~0.6 it barely deviates from the sphere
      union underneath. */
  largeFrequency: 0.8,
  /** How far the large scale can push the surface, in world units. This is
      the amplitude that decides whether a sphere is still visible in the
      silhouette. At 0.5+ the marcher starts overstepping badly (see
      `march.stepScale`) and the edges tear. */
  largeAmplitude: 0.52,

  /** The crease scale, and the one that decides whether the surface reads as
      liquid or as terrain. It is sampled *ridged* — one minus the absolute
      value of the noise — which puts sharp creases where plain noise puts
      round bumps. The reference is full of them: the folds meet at edges,
      not at shoulders. */
  mediumFrequency: 2.4,
  mediumAmplitude: 0.16,

  /** Where the ridged scale sits before it is scaled, so the creases push out
      and the flats pull in rather than the whole surface inflating. Roughly
      the mean of the ridged sum; away from it the mass gains or loses volume
      overall instead of gaining creases. */
  ridgeBias: 0.46,

  /* There used to be a third, fine scale here, at frequency 7.4 and
     amplitude 0.022. It was deleted rather than turned down: at that
     amplitude it moved the surface by two hundredths of a unit — far below
     what the silhouette can show — while costing a full FBM evaluation
     inside the march loop, which is the most expensive place in the project
     to spend anything. The micro-detail it was meant to provide comes from
     the normal map, at the price of one texture fetch on surfaces the ray
     has already hit. */

  /** Octaves inside each FBM call. Every octave runs at every march step, so
      this multiplies against `march.maxSteps` and against the number of FBM
      calls per step — the single most expensive number in the file. Two is
      enough once domain warping is doing the structural work; three cost
      about half again as much for detail the warp already implies. */
  octaves: 2,

  /** How fast the field evolves. The matter should read as existing rather
      than as animating: at 0.1 it visibly churns, at 0.02 the movement is
      only noticeable if you look away and back. */
  timeScale: 0.035,
} as const

export const warp = {
  /** Domain warping is what separates folds and flows from a bumpy ball. The
      position is displaced by a low-frequency noise before the main FBM is
      sampled, so the fractal's own coordinate space swirls.

      Strength is in world units. Above ~0.9 the warp folds the field back
      over itself and the marcher finds surfaces where there is no coherent
      normal — the result is speckle. Below ~0.25 the effect reads as a
      slightly wobbly sphere and the whole point is lost. */
  strength: 0.9,
  /** Deliberately lower than `noise.largeFrequency`: the warp has to be
      broader than the thing it distorts, or it just adds another noise
      octave instead of reorganising the field. */
  frequency: 0.42,
  /** The warp drifts slower than the surface it distorts, so folds appear to
      travel through the matter rather than the whole mass shimmering. */
  timeScale: 0.018,
} as const

/* ── Raymarching ─────────────────────────────────────────────────────────── */

export const march = {
  /** Upper bound on steps per pixel. The frame cost is roughly
      maxSteps × octaves × pixels, and this is the lever the quality ladder
      pulls first after render scale. Below ~48 the silhouette's grazing
      edges — where rays travel nearly parallel to the surface — dissolve
      into noise. */
  maxSteps: 56,

  /** How close counts as a hit, in world units. Too large and the surface
      gains a soft halo of near-misses; too small and grazing rays burn their
      whole step budget without converging. */
  epsilon: 0.0016,

  /** Ray distance at which we give up and return background. The arms reach
      about 1.8 from the origin and the camera starts 6.3 out, so anything
      past this is empty space in every direction. */
  maxDistance: 14.0,

  /**
   * Offsetting a distance field by noise breaks the Lipschitz guarantee that
   * makes sphere tracing safe: the true distance can be shorter than the
   * field reports, and a full step then jumps straight through the surface,
   * punching holes in the silhouette.
   *
   * Multiplying every step by this recovers correctness the cheap way. It is
   * roughly 1/(1 + total noise gradient); with the amplitudes above, 0.5 is
   * where the tearing stops. Raising it toward 1 brings the holes back —
   * first as sparkle on the fold edges, then as gaps. Lowering it is safe but
   * pure cost: every 0.1 removed is about 15% more steps for the same image.
   */
  stepScale: 0.5,

  /** Offset used to sample the field for the surface normal. Must be larger
      than `epsilon` — sampling closer than the hit tolerance returns the same
      value four times and the normal collapses to zero. Larger values smooth
      the shading, which quietly erases the small noise scale. */
  normalEpsilon: 0.004,
} as const

/* ── Material ────────────────────────────────────────────────────────────── */

export const material = {
  /** World-space size of one texture tile. The maps are 512px of fine grain;
      at 0.9 units a tile spans about half a lobe, which keeps the grain
      sub-pixel enough not to read as a pattern. Below ~0.4 the tiling repeat
      becomes visible on the broad front faces. */
  triplanarScale: 0.9,

  /** Exponent on the triplanar blend weights. Low values (1–2) cross-fade the
      three projections over a wide band and turn the grain to mush on any
      surface facing diagonally — which, on a blobby body, is most of it. High
      values (8+) narrow the band until the projection boundaries show as
      seams. */
  triplanarSharpness: 4.0,

  /** How much of the normal map is applied. This is micro-relief only; the
      real form comes from the field. Above ~0.6 the grain starts to fight the
      folds for the eye and the surface reads as pebbled rather than wet. */
  normalStrength: 0.35,

  /** Roughness range the roughness map is remapped into. Wet ink lives at the
      low end — this is what makes the surface read as liquid rather than
      matte rubber. The spread matters more than the midpoint: a constant
      roughness looks synthetic no matter what value it holds. */
  roughnessMin: 0.08,
  roughnessMax: 0.34,

  /** Base colour of the body. Almost black on purpose: everything a viewer
      sees on this surface is reflection, scatter or vein, and a lighter base
      washes all three out at once. */
  baseColor: palette.nightVoid,

  /** How strongly the environment map lights the surface. This is the whole
      photoreal budget: at 0 the object is a silhouette-shaped hole, at 2+ it
      turns into polished chrome and stops being organic. */
  envIntensity: 1.15,

  /** Fresnel rim. `power` is the falloff exponent — lower spreads the rim
      across the whole body and produces the blue-shell failure PLAN.md warns
      about; higher confines it to the true grazing edge. */
  fresnelPower: 3.4,
  fresnelStrength: 0.55,
  fresnelColor: palette.nightRim,
} as const

/* ── Crevice glow ────────────────────────────────────────────────────────
   Where the reference's blue actually lives: not around the silhouette but
   down in the gaps between masses, brightest where two folds nearly touch.
   Measured with an ambient-occlusion probe along the normal — the field is
   closer than the probe distance exactly where the surface is enclosed. */

export const glow = {
  /** Probe steps. Three is enough to tell a gap from an open face; each one
      is a full evaluation of the field, on hit pixels only. */
  samples: 3,
  /** Spacing of the probe, in world units. Comparable to the width of the
      gaps worth lighting — too far and every concave patch glows, too near
      and only hairline cracks do. */
  spacing: 0.11,

  /** Overall brightness of the crevice light. */
  strength: 2.6,
  /** Exponent on occlusion. High values confine the light to the deepest
      cracks, which is what keeps it from washing the whole body blue. */
  falloff: 2.4,

  /** Ramps from electric in ordinary gaps to cyan in the deepest, which is
      the reference's own distribution — cyan is rare and always at maximum
      energy. */
  deepColor: palette.electric,
  hotColor: palette.cyanNeon,
} as const

/* ── Subsurface scatter ──────────────────────────────────────────────────
   The membrane read: light through the thin trailing edges of the arms,
   where they taper to nothing. Secondary to the crevice glow now, and much
   weaker than it was when it was carrying the whole scene's colour. */

export const sss = {
  /** How far the thickness probe reaches, in world units. Beyond the body's
      own thickness this measures nothing and only costs steps. */
  probeDistance: 0.9,
  /** Samples along the probe. Four is enough to distinguish a fin from a
      body; more just smooths a value that is already an approximation. */
  probeSamples: 3,

  /** Overall scatter brightness. The first thing to turn down if the hero
      reads as "glowing blue thing" instead of "dark matter". */
  strength: 0.7,

  /** Exponent applied to normalised thinness. Above ~3 the glow retreats to
      a hairline on the very thinnest fins and the membrane read is lost;
      below ~1.2 the whole body lights up from within like a lamp. */
  falloff: 2.1,

  /** Scatter colour ramps from `deep` in merely-thin regions to `thin` where
      the membrane is nearly transparent. The cyan end is reserved for the
      extremes, which is what keeps it rare. */
  deepColor: palette.electric,
  thinColor: palette.cyanNeon,
} as const

/* ── Electric veins ──────────────────────────────────────────────────────── */

export const veins = {
  /** Vein noise frequency. These are filaments inside the matter, so this is
      well above the fold scale — otherwise the veins follow the folds exactly
      and read as edge lighting. */
  frequency: 3.8,

  /**
   * Veins are the narrow band of a noise field around a threshold. `width` is
   * the half-width of that band.
   *
   * This pair decides how much blue the scene carries, which the reference
   * image sets. Wider (> 0.1) and the filaments thicken into glowing patches;
   * narrower (< 0.02) and they break into disconnected sparks.
   */
  threshold: 0.5,
  width: 0.055,

  /** A second, much broader noise gates whole regions of veins on and off, so
      they appear in some places and are absent in others rather than evenly
      covering the body. */
  maskFrequency: 0.9,
  maskBias: 0.42,

  /** Veins drift faster than the matter they sit in — they are the one thing
      in the scene allowed to read as electrical rather than geological. */
  timeScale: 0.11,

  /** Emissive strength. This is what Bloom's threshold is tuned against: the
      veins are the only surface in the scene meant to exceed it. */
  intensity: 2.4,

  /** Vein colour ramps with energy. Cyan only at the very top, which is the
      "very small areas of maximum energy" rule from PLAN.md. */
  coreColor: palette.electric,
  hotColor: palette.electricLift,
  peakColor: palette.cyanNeon,
} as const

/* ── The eye ─────────────────────────────────────────────────────────────
   One eye, unioned into the same field as the body — not a mesh laid on top,
   so the folds occlude it through the same distance function that made
   them. */

export const eye = {
  /** Sits inside a fold on the camera-facing side, below the mass's centre.
      Moving it outward past the surface turns it into a bauble stuck to the
      front; too far in and the body swallows it completely. */
  /* The magnitude of this vector matters more than its direction: the body's
     undeformed radius is 1, so an eye centred at 0.75 with radius 0.17
     reached only 0.92 and was swallowed whole — which is exactly what the
     first build showed. It is measured against the core, which is now 0.72,
     so a centre at 0.66 puts it at that surface — the relief opens over it in
     some places and closes over it in others. */
  position: [0.14, -0.08, 0.66] as const,

  /** Radius. Small enough that it is discovered rather than presented. */
  radius: 0.17,

  /** How much smaller the iris is than the eyeball, as a fraction. */
  irisScale: 0.52,
  pupilScale: 0.22,

  /** Blend radius where the eye meets the body. Sharp (< 0.02) reads as a
      ball dropped into a hole; soft (> 0.1) and the eye's own silhouette
      dissolves and it stops being an eye. */
  smoothness: 0.05,

  /** Emissive strength of the iris. The reference makes the eye legible, so
      this is not the near-zero of the first draft — but it stays under
      `veins.intensity`, so the eye can never be the brightest thing on
      screen. */
  irisIntensity: 0.9,
  irisColor: palette.electric,
  pupilColor: palette.nightVoid,
  scleraColor: palette.nightDeep,

  /** Maximum rotation toward the pointer, in radians. About 3.5°: enough
      that a viewer might catch it, little enough that they cannot be sure. */
  trackAmount: 0.06,
  /** Per-second approach rate of the eye toward its target. Slower than the
      body's, so the eye lags the mass and the movement reads as intent
      rather than as parallax. */
  trackDamping: 1.4,
} as const

/* ── Motion ──────────────────────────────────────────────────────────────── */

export const motion = {
  /** Breathing: a scale oscillation with an amplitude a viewer should never
      be able to point at. Above ~0.02 it reads as a heartbeat. */
  breathAmplitude: 0.008,
  /** Period in seconds. Long enough that it never syncs with the pointer
      damping or the noise drift into a visible beat. */
  breathPeriod: 11.0,

  /** Idle rotation, radians per second. Not a turntable — this is drift. */
  rotationSpeed: 0.014,
} as const

export const pointer = {
  /** Maximum rotation of the whole mass toward the pointer, in radians. */
  rotateAmount: 0.13,
  /** Exponential approach rate per second. At 6+ the mass snaps and the
      illusion of mass is gone; at 1 it lags so far it reads as broken. */
  damping: 2.6,
  /** Pointer parallax on the internal highlights, as a fraction of the
      body radius. */
  highlightShift: 0.12,
} as const

/* ── Camera and scroll ───────────────────────────────────────────────────── */

export const camera = {
  fov: 38,
  near: 0.1,
  far: 24,

  /** Where the camera starts, at scroll progress 0. The X offset frames the
      mass in the right half of the viewport, leaving the left for the
      headline — the composition PLAN.md specifies. */
  startPosition: [-0.6, 0.1, 6.3] as const,
  /** Where it ends, at progress 1: forward and slightly up, passing beside
      the mass rather than into it. */
  endPosition: [0.3, 0.4, 0.5] as const,

  /** The camera keeps looking at the body through the whole move, so the
      mass swings across frame as the camera passes it. */
  target: [0, 0, 0] as const,
} as const

export const scroll = {
  /** How many viewport heights the hero is pinned for. Two is the shortest
      pin that gives the approach room to read as an approach; three starts to
      feel like the visitor is being held. */
  screens: 2,

  /** Progress at which the camera reaches the particle cloud. Before this the
      shot is an approach; after it, a pass-through. */
  particleEntry: 0.62,

  /** Progress at which depth of field starts. PLAN.md forbids DOF while the
      hero is the hero — this is the tail, where the scene is handing over to
      the section below. */
  dofStart: 0.85,

  /** Progress at which the canvas has fully dissolved into the page ground.
      Ends before 1 so the last sliver of scroll is settled black rather than
      a still-fading image. */
  fadeEnd: 0.96,
} as const

/* ── Particles ───────────────────────────────────────────────────────────── */

export const particles = {
  /** Count at full quality. These exist for scale and for the pass-through;
      they are not the subject. */
  count: 900,

  /** Seed for the deterministic generator. Fixed so the arrangement is the
      same on every load and can be tuned by eye — and so tests can assert
      against it. */
  seed: 0x5eed1a,

  /** Radius of the shell the particles occupy, and how far in from it they
      may sit. The cloud has to be a shell rather than a ball: the camera
      flies through it, and particles at the centre would be inside the
      matter. */
  shellRadius: 2.6,
  shellThickness: 1.4,

  /** Size in *world units*, not pixels — the material attenuates by distance,
      which makes this a physical diameter. At 2.2 (the pixel-sized value this
      replaced) every particle was the size of the matter itself and the
      screen filled with squares. A particle is a mote: sub-centimetre against
      a body two units across. */
  size: 0.02,

  /** Ceiling on particle opacity. They exist for scale and for the moment the
      camera passes through them; anything a viewer can consciously count is
      already too strong. */
  opacity: 0.32,

  /** Drift speed, world units per second. Slow enough to read as suspended
      rather than falling. */
  drift: 0.018,

  /** Fraction lit by cyan rather than left near-black. The rest are barely
      above the background — which is the point: they register as space, not
      as a particle system. */
  litFraction: 0.08,
  dimColor: palette.nightEdge,
  litColor: palette.cyanNeon,
} as const

/* ── Post-processing ─────────────────────────────────────────────────────── */

export const post = {
  /** Luminance above which Bloom takes hold. Tuned to sit just under the
      veins' emissive peak and above every lit surface, which is what makes
      the bloom selective without a separate pass. Lowering it is the fastest
      way to make the whole hero glow, which is the failure mode PLAN.md
      names. */
  bloomThreshold: 0.62,
  bloomSmoothing: 0.28,
  bloomIntensity: 0.85,

  /** Depth of field at the tail of the scroll. Only ever reached past
      `scroll.dofStart`. */
  dofFocusDistance: 0.02,
  dofFocalLength: 0.05,
  dofBokehScale: 3.2,
} as const

/* ── Quality ─────────────────────────────────────────────────────────────── */

export type QualityTier = 'full' | 'reduced' | 'low' | 'still'

export const quality = {
  /** Frames averaged before a tier change is considered. Short windows react
      to a single stutter — a garbage collection or a scroll — and drop
      quality nobody needed to lose. Ninety frames is about 1.5s at 60fps. */
  sampleWindow: 90,

  /** Below this average, drop one tier. Set under 60 rather than at it so a
      display that simply runs at 50 does not trigger a downgrade. */
  downgradeFps: 45,

  /** The ladder only ever descends, and only one step at a time. Recovering
      upward would oscillate: the higher tier is what caused the low frame
      rate, so restoring it re-triggers the drop, forever. */
  tiers: {
    /* Full quality already renders below the display's own resolution. For a
       marcher the pixel count is the whole cost, and this scene — dark, soft,
       almost without hard edges — loses less to a 15% reduction than any
       other lever gives back. */
    full: { renderScale: 0.85, maxSteps: march.maxSteps, sss: true, bloom: true },
    reduced: { renderScale: 0.7, maxSteps: 44, sss: false, bloom: false },
    low: { renderScale: 0.4, maxSteps: 32, sss: false, bloom: false },
    still: { renderScale: 0.85, maxSteps: march.maxSteps, sss: true, bloom: true },
  },

  /** Upper bound on device pixel ratio, before `renderScale`. A phone at
      DPR 3 would otherwise ask the marcher for nine times the pixels of a
      DPR-1 display for detail this scene — dark, soft, low-contrast — cannot
      show. */
  maxDpr: 1.5,
} as const

/** The still tier renders exactly one frame and then stops the loop. There is
    no pre-rendered image to keep in sync with the shader, and a frame of the
    real scene can never disagree with it. */
export const stillTierRendersOneFrame = true

export const assets = {
  environment: '/env/studio-1k.hdr',
  normalMap: '/textures/membrane/normal.webp',
  roughnessMap: '/textures/membrane/roughness.webp',
} as const
