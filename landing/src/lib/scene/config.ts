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
   A union of spheres, welded by a polynomial smooth-minimum. The spheres are
   scaffolding: nothing here should be recognisable as a sphere in the final
   image, which is the job of `smoothness` below and of the noise on top. */

export const body = {
  /** Radius of the core sphere, in world units. Everything else is sized
      against this, and the camera framing in `camera` assumes it. */
  coreRadius: 1.0,

  /**
   * The satellites, each `{ position, radius, squash }`. Deliberately not
   * symmetric and deliberately not evenly spaced: a symmetric arrangement
   * survives the noise and reads as an ornament, which is the one thing an
   * organic mass must never look like.
   *
   * `squash` scales the sphere's local Y before the distance is taken, which
   * is what turns a ball into a lobe. Values far from 1 start to show the
   * ellipsoid's own axis, so they stay inside roughly 0.55–1.5.
   */
  lobes: [
    { position: [0.78, 0.42, -0.18], radius: 0.62, squash: 0.72 },
    { position: [-0.66, 0.58, 0.24], radius: 0.55, squash: 1.28 },
    { position: [-0.34, -0.72, -0.3], radius: 0.68, squash: 0.85 },
    { position: [0.46, -0.58, 0.42], radius: 0.48, squash: 1.1 },
    { position: [0.12, 0.86, 0.36], radius: 0.4, squash: 0.9 },
  ] as ReadonlyArray<{
    position: readonly [number, number, number]
    radius: number
    squash: number
  }>,

  /**
   * The `smin` blend radius. This is the single most character-defining
   * number in the file.
   *
   * Too low (< 0.2) and the lobes read as separate balls stuck together —
   * visible waists between them, which is exactly the seam problem that made
   * us abandon meshes in the first place. Too high (> 0.6) and every lobe
   * dissolves into the core: the silhouette rounds off into one blob and the
   * whole reason for having satellites disappears.
   */
  smoothness: 0.38,
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
  largeFrequency: 0.85,
  /** How far the large scale can push the surface, in world units. This is
      the amplitude that decides whether a sphere is still visible in the
      silhouette. At 0.5+ the marcher starts overstepping badly (see
      `march.stepScale`) and the edges tear. */
  largeAmplitude: 0.34,

  /** The fold scale. Frequency roughly 3× the large scale so the two do not
      beat against each other into a regular pattern. */
  mediumFrequency: 2.6,
  mediumAmplitude: 0.11,

  /** Micro-relief. Deliberately weak: this is the scale that turns matter
      into stone if it is allowed to compete. Most of the fine detail a viewer
      actually sees comes from the normal map, which costs one texture fetch
      rather than a whole octave inside the march loop. */
  smallFrequency: 7.4,
  smallAmplitude: 0.022,

  /** Octaves inside each FBM call. Every octave is evaluated at every march
      step, so this multiplies against `march.maxSteps` — the single most
      expensive number in the file. Three is where the surface stops looking
      mathematically clean; four is barely distinguishable and costs a third
      more. */
  octaves: 3,

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
  strength: 0.55,
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
  maxSteps: 72,

  /** How close counts as a hit, in world units. Too large and the surface
      gains a soft halo of near-misses; too small and grazing rays burn their
      whole step budget without converging. */
  epsilon: 0.0016,

  /** Ray distance at which we give up and return background. The body is
      ~2 units across sitting ~4.2 from the camera, so anything past this is
      empty space. */
  maxDistance: 12.0,

  /**
   * Offsetting a distance field by noise breaks the Lipschitz guarantee that
   * makes sphere tracing safe: the true distance can be shorter than the
   * field reports, and a full step then jumps straight through the surface,
   * punching holes in the silhouette.
   *
   * Multiplying every step by this recovers correctness the cheap way. It is
   * roughly 1/(1 + total noise gradient); with the amplitudes above, 0.62 is
   * where the tearing stops. Raising it toward 1 brings the holes back —
   * first as sparkle on the fold edges, then as gaps. Lowering it is safe but
   * pure cost: every 0.1 removed is about 15% more steps for the same image.
   */
  stepScale: 0.62,

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

/* ── Subsurface scatter ──────────────────────────────────────────────────
   What makes it a membrane rather than a solid. Thickness is estimated by
   marching back into the field from the hit point toward the light; thin
   places glow. This is also where nearly all of the scene's blue comes
   from. */

export const sss = {
  /** How far the thickness probe reaches, in world units. Beyond the body's
      own thickness this measures nothing and only costs steps. */
  probeDistance: 0.9,
  /** Samples along the probe. Four is enough to distinguish a fin from a
      body; more just smooths a value that is already an approximation. */
  probeSamples: 4,

  /** Overall scatter brightness. The first thing to turn down if the hero
      reads as "glowing blue thing" instead of "dark matter". */
  strength: 1.25,

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
  position: [0.16, -0.12, 0.72] as const,

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
  startPosition: [-0.55, 0.08, 4.2] as const,
  /** Where it ends, at progress 1: forward and slightly up, passing beside
      the mass rather than into it. */
  endPosition: [0.22, 0.34, 0.35] as const,

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

  /** Size in pixels at full quality, before the distance attenuation. */
  size: 2.2,

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
    full: { renderScale: 1.0, maxSteps: march.maxSteps, sss: true, bloom: true },
    reduced: { renderScale: 0.8, maxSteps: 56, sss: false, bloom: false },
    low: { renderScale: 0.4, maxSteps: 40, sss: false, bloom: false },
    still: { renderScale: 0.6, maxSteps: march.maxSteps, sss: true, bloom: true },
  },

  /** Upper bound on device pixel ratio, before `renderScale`. A phone at
      DPR 3 would otherwise ask the marcher for nine times the pixels of a
      DPR-1 display for detail this scene — dark, soft, low-contrast — cannot
      show. */
  maxDpr: 2,
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
