/**
 * Every number in the scene lives here.
 *
 * This is not a matter of style but a direct consequence of why the previous
 * attempt failed: you cannot choose what a stone looks like on paper, you
 * choose it by the loop "change a number → take a frame". The loop requires the
 * numbers to sit in one place and to be named for what they mean. No scene
 * constant should ever appear inside a component.
 */

import type { FbmOptions } from './noise'

/** Quality tiers. The ladder is one-way: down only. */
export type Tier = 'full' | 'reduced' | 'low' | 'still'

export const TIERS: readonly Tier[] = ['full', 'reduced', 'low', 'still']

export const CUBE = {
  /** Edge length in scene units. The half-extent, size / 2, is used everywhere as h. */
  size: 2,

  /**
   * How far the cube's centre sits above the point the camera looks at.
   *
   * The cube has to hang in the upper part of the frame with the copy beneath
   * it. The rise is expressed in scene units rather than in percentages of the
   * screen, because the relation between the two is set by `CAMERA`: at the
   * resting distance the visible frame height is
   * `2 · |rest| · tan(fov/2) ≈ 4.55`, so 0.68 is roughly 15% of the frame
   * height above centre.
   */
  centerY: 1.14,
} as const

/**
 * The camera at rest.
 *
 * The cut planes are X = 0 and Y = 0, so the cross of fissures is drawn on the
 * ±Z faces, while the sides and the lid each show a single line. The camera
 * therefore has to look predominantly along Z, slightly to the side and from
 * above: it is the only setup in which the division into four reads at a
 * glance.
 *
 * The resting distance follows from how large the cube should be in frame: at
 * `fov = 30°` and `|rest| ≈ 8.5` the visible frame height at the cube is
 * ≈ 4.55 units, and a cube of height 2 takes up ≈ 44% — roughly the 40% the
 * object occupies in the reference. An azimuth of ≈ 17° and an elevation of
 * ≈ 12° give a soft three-quarter view: the front face with the cross
 * dominates, while the side and the lid show as strips.
 */
export const CAMERA = {
  fov: 30,
  rest: [3.24, 2.3, 10.8] as const,
  target: [0, 0, 0] as const,
} as const

/**
 * Life at rest: the stone balances, it does not spin.
 *
 * A full rotation was rejected. It produced two azimuths where the faces
 * carrying the cross swung out of view and — more importantly — a rotation
 * reads as a display case: an object showing itself off from every side, like
 * goods on a turntable. Balancing reads as mass, hanging and searching for
 * equilibrium, and that is what the stone has to be.
 *
 * Three axes sway with incommensurable periods, so the combined motion never
 * repeats and is not perceived as a loop. The amplitudes are deliberately
 * small: on a rough surface a highlight jumps from cavity to cavity even under
 * a slight sway, and that is what proves the material.
 */
export const BALANCE = {
  /** Resting pose in radians: angle to the camera, tilt and roll. */
  pose: [-0.2, 0.62, 0.12] as const,

  /** Sway amplitudes on the three axes, radians. */
  amplitude: [0.055, 0.075, 0.035] as const,
  /** Periods, seconds. Incommensurable: they never come back into phase. */
  period: [13, 19, 23] as const,

  floatAmplitude: 0.04,
  floatPeriod: 11,
} as const

/**
 * The fracture: the field that sculpts the stone's surface, the mating
 * surfaces and the chips on the edges alike. One field for all of it — because
 * the interlocking of the parts rests on exactly this, that the displacement
 * is a function of the point and of nothing else.
 */
export const FRACTURE = {
  seed: 1337,

  /** Noise cycles per scene unit. The coarseness of the jags. */
  frequency: 1.35,

  /** Displacement amplitude in scene units, over the smooth part of the surface. */
  amplitude: 0.045,

  fbm: {
    octaves: 5,
    lacunarity: 2.07,
    gain: 0.52,
    ridged: 0.55,
  } satisfies FbmOptions,

  /**
   * Chips on the edges.
   *
   * `gain` — how many times larger the amplitude is on the edge itself than on
   * the face. `width` — the fraction of the half-extent over which it ramps
   * up. `cornerGain` — the extra at corners, where three faces meet.
   *
   * Chips have to be coarser than the surface grain, or the silhouette comes
   * out fuzzy rather than chipped. Hence their own, lower frequency.
   */
  edge: {
    gain: 2.6,
    width: 0.18,
    cornerGain: 1.4,
    frequencyScale: 0.45,
  },

  /**
   * Seam meander.
   *
   * Without it the fissure on the outer surface is nearly straight: only the
   * general grain leads it, and that is 2% of an edge — a few pixels on
   * screen. Stone does not break that way; a fracture follows the weak places
   * in the rock and wanders.
   *
   * The field is its own, with a larger amplitude and a lower frequency, and
   * it only switches on near the cut planes. It still depends on the point
   * alone, so the interlocking of the parts is unharmed: both sides of the
   * seam wander identically.
   */
  seam: {
    gain: 6.5,
    width: 0.2,
    /**
     * The meander's frequency. It works out as `frequency × frequencyScale`
     * cycles per scene unit, and the cube has size 2. So the whole fissure
     * spans `2 × frequency × frequencyScale` periods, and below roughly one
     * and a half it stops wandering and merely drifts to one side.
     */
    frequencyScale: 0.65,
    /** Displacement along the fissure relative to displacement along the cut plane's normal. */
    alongShare: 0.35,
  },
} as const

/**
 * The gap between the parts at rest.
 *
 * Each part is pushed away from the centre by half this value along its own
 * diagonal, so the total clearance across each cut plane equals `rest`. Zero
 * is not allowed: at rest the fissure is the only source of blue in the frame.
 */
export const GAP = {
  /**
   * How far each part is pushed from the centre along the pure diagonal. The
   * upward bias is not included here — that belongs to the gesture, and the
   * resting gap has to be symmetrical.
   *
   * Hairline and no more: at rest the stone has to read as one solid piece,
   * not as something assembled from blocks. Zero is still not allowed —
   * coincident fracture surfaces would give z-fighting all along the mating
   * surface.
   */
  rest: 0.004,

  /** Brightness breathing at rest: a fraction of the base emissive, and a period in seconds. */
  breathAmplitude: 0.22,
  breathPeriod: 7.5,
} as const

/**
 * The stone opening under the cursor.
 *
 * **All four parts rise at once**, not the one the cursor is over. The gesture
 * is not about picking a quarter; it is about the stone coming apart along its
 * fractures, and then it does not matter where exactly the pointer landed —
 * what matters is that it is on the stone.
 *
 * A side effect argues for the decision: all four move apart, so the clearance
 * across each cut plane is contributed to from both sides and reads twice as
 * wide as the same displacement of a single part would give.
 */
export const HOVER = {
  /** Each part's outward displacement along the diagonal, in scene units. */
  lift: 0.16,

  /**
   * How far the diagonal is biased upward: 0 is the pure diagonal, 1 is
   * straight up. The brief asked for the parts to "rise", and a small bias
   * gives that without breaking the proof that neighbours cannot intersect.
   */
  upBias: 0.25,

  /**
   * Attack and release in seconds — these are the exponential's `tau`, the
   * time in which the remainder shrinks by a factor of `e`, not the duration
   * of the gesture: it completes in roughly three `tau`.
   *
   * The release is noticeably longer than the attack. The stone has to answer
   * immediately but settle heavily — mass that comes down as briskly as it
   * went up reads as empty.
   */
  attack: 0.5,
  release: 0.95,
} as const

/**
 * The core inside the stone.
 *
 * The light lives on the fracture surfaces, and its brightness rises **toward
 * the cube's centre**: that reads as a source in the depths rather than as
 * paint along the seams. Near the surface only `mouthLevel` remains — just
 * enough for the seam to be sketched in light at rest, and no more.
 *
 * The intensification when a part lifts then comes for free: as it moves away,
 * the part exposes a deeper — and therefore brighter — band of surface to the
 * camera. There is no separate flash to animate, so light and motion cannot
 * fall out of step.
 */
export const GLOW = {
  /** The hottest core — cyan-neon from the page palette. */
  core: '#2aa3ad',
  /** The main tone of the glow — electric. */
  body: '#1a4fa0',

  /**
   * Raised along with the palette's cooling, and that is a condition rather
   * than a compensation.
   *
   * Neither colour above is native: both are taken from `cyan-neon` and
   * `electric`, and when the palette moved to deep cobalt and icy cyan the
   * glow moved with it. But this is **emission**, not paint: it has to clear
   * the bloom threshold at 0.66, and what reads as "more restrained" in the
   * markup means "below threshold" here — the fissure would simply stop
   * blooming. The multiplier returns the emissive to its former level, leaving
   * only the tone and the coldness new.
   */
  intensity: 1.35,

  /**
   * Emissive right at the mouth of the fissure, as a fraction of the core.
   * Deliberately low.
   *
   * In the reference the inside of the fracture is nearly black: the light
   * goes deep, and what shows on the outside is not a slot but **a lit lip of
   * the break**. A high `mouthLevel` gave an even neon tube — precisely what a
   * real fissure is not.
   */
  mouthLevel: 0.16,
  /** How sharply the light concentrates toward the centre. Higher hides it deeper. */
  concentration: 4.4,

  /** Emissive multiplier at rest and at a fully lifted part. */
  restGain: 0.34,
  hoverGain: 1,

  /** Peak energy of the travelling pulse at the moment of the lock. */
  lockPulse: 3.2,

  /**
   * Veins: light leaves the fracture along the rock's real cracks, fading with
   * distance. This is what separates a photograph from a drawn outline — in a
   * photograph the main fracture is not alone, a network with junctions and
   * dead ends runs off it.
   *
   * `veinReach` — the fraction of the half-extent over which the network burns
   * out.
   */
  veinReach: 0.22,
  /**
   * Zero on purpose. The mechanism stays: up close — in the canyon frame,
   * where the fissure fills the screen — veins will read as veins. At the
   * resting distance their cells are smaller than a few pixels, and instead of
   * a network you get shimmer.
   */
  veinIntensity: 0,

  /**
   * The spill across the stone — the main carrier of the glow, rather than the
   * gap itself.
   *
   * In the reference the blue lies on the slab: a narrow hot lip at the break
   * and a soft falloff over a few centimetres of surface. Physically this is
   * light from the fracture landing on the rock; here it is expressed as
   * emissive on the outer surface, because there is no real source in the
   * scene (see the decision about the core).
   *
   * `spillFalloff` is the falloff exponent. Above one it presses the light
   * against the lip itself and leaves the slab dark, as in the photograph; an
   * even linear falloff would give a broad washed-out wash.
   */
  spillWidth: 0.055,
  spillIntensity: 0.85,
  spillFalloff: 1.5,
} as const

/**
 * Light.
 *
 * The environment comes from a compact studio HDRI; the directional sources on
 * top of it carry the relief and preserve the cold staging of the source
 * scene.
 *
 * `exposure` and `envIntensity` are two different knobs and must not be
 * confused. `envIntensity` changes how much light reaches the stone (that is,
 * the contrast between the lit and the shadowed face); `exposure` changes how
 * the computed brightness lands in the frame. A dark scene is fixed by the
 * first; the second only lifts the black background along with the stone.
 */
export const LIGHT = {
  /* Lowered by one more quiet step: this is a knob on the frame, not on the
     light falling on the stone, so it seats the whole picture while preserving
     the contrast and the relief of the faces. */
  exposure: 0.92,
  envIntensity: 0.65,

  /**
   * Three directional sources on top of the environment.
   *
   * The environment alone does not carry the stone, and that is physics rather
   * than a setting: the albedo of dark rock is around 0.1, and the diffuse
   * part of IBL from a few small glowing rectangles is soft ambient light.
   * Dark on ambient reads as a silhouette, not as stone.
   *
   * Directional light gives what IBL cannot: **a terminator**. The hard
   * boundary of light and shadow creeping across the cavities is the only
   * thing that makes the relief legible, and it is exactly what works in the
   * reference.
   */
  sun: {
    intensity: 5.6,
    position: [3.4, 3.0, 2.2],
    color: '#a8ccff',
  },
  /**
   * Fill from the left. Not "weak for looks" but mandatory: the shadow side
   * has to keep some rock in it. A fully black side turns the cube into a flat
   * lit face with dark wings, and the mass disappears.
   */
  fill: {
    intensity: 2.4,
    position: [-3.6, -0.4, 2.2],
    color: '#3b6ba8',
  },
  /** From above, separate from the rim: without it the lid does not detach from the front. */
  top: {
    intensity: 1.8,
    position: [-0.4, 4.0, 0.8],
    color: '#9dbde6',
  },
} as const

/**
 * Post-processing.
 *
 * `bloom` is not decoration but the thing that turns emissive into light:
 * without it the core stays a bright pixel on the fracture surface and never
 * spills onto the stone around it. The threshold sits above the stone's own
 * emissive so that only the fissure blooms.
 *
 * `grain` is there against banding: a dark blue gradient around the cube at
 * eight bits per channel produces visible stepped rings, and grain breaks them
 * up. This is not a stylistic choice.
 *
 * Defocus and aberration only exist over the last few percent of the pin;
 * their numbers live in `SCROLL` because they are choreography, not look.
 */
export const POST = {
  bloom: {
    intensity: 1.15,
    threshold: 0.66,
    smoothing: 0.28,
    /** How widely the halo spreads. Larger is softer and more expensive. */
    radius: 0.72,
  },
  vignette: {
    offset: 0.28,
    darkness: 0.62,
  },
  grain: {
    opacity: 0.045,
  },
} as const

/**
 * Scroll choreography.
 *
 * A 200vh pin: 150 for the approach with the opening, plus half a viewport for
 * the darkening, which must not be rushed — that is where the handoff to the
 * sections below happens, and if it is skipped past the visitor gets to watch
 * the canvas go out.
 *
 * The spans are given in **scene** progress, not pin progress: between them
 * sits the density map, which hands the opening the extra scroll. That is why
 * 0.6–0.85 in scene terms occupies appreciably more than a quarter of the pin.
 */

/**
 * The camera approach span.
 *
 * Declared separately because it is referenced twice: it drives the camera and
 * it drives the copy's fly-past. Two equal numbers in two fields would drift
 * apart at the first edit, and they must not — see `SCROLL.copy`.
 */
const APPROACH = { from: 0.1, to: 0.65 } as const

export const SCROLL = {
  /** Pin length. A string for ScrollTrigger. */
  pin: '+=200%',
  /**
   * Scrub smoothing, seconds. A mouse wheel is a fifty-pixel jump with no
   * intermediate values, and without smoothing the scene has nothing to draw
   * in between.
   */
  scrub: 0.5,

  /**
   * The copy's fly-past. **The very same number** as the approach, and that is
   * the whole point of it.
   *
   * The copy does not leave and does not fade — it stands in the same space as
   * the stone, and the camera drives through it. Which means its motion has to
   * be that same camera approach: a duration of its own would mean the text
   * and the stone live in two different spaces that happen to be overlaid. And
   * that is exactly what used to read as "the text disappeared" — it faded on
   * its own clock while the camera was still only starting to move.
   */
  copy: APPROACH,
  approach: APPROACH,
  opening: { from: 0.6, to: 0.78 },
  /** The cube opens fully, then the guardrail snaps the parts back inward. */
  lock: { from: 0.78, to: 0.88 },
  /** A wave runs along the fracture at the same time as the lock. */
  pulse: { from: 0.78, to: 0.9 },
  /** The camera resumes only after the lock's impact has read. */
  handoff: { from: 0.86, to: 0.96 },

  /**
   * The exit. It ends **before** the pin does, and that is the whole point of
   * it.
   *
   * It used to be `to: 1`, meaning the curtain reached black exactly on the
   * pin's last pixel. Measuring with the wheel (120 px per notch) shows how
   * that ends: at 96.7% of the pin the curtain holds 0.764, the pin releases
   * at 100%, and the curtain reaches one at 103.3% — that is, **sixty pixels
   * after the page has already started moving**. The visitor watches a lit
   * canyon at a quarter opacity turn into a flat sheet and slide upward. This
   * is precisely the seam that is supposed to be invisible.
   *
   * Zero headroom could not have been chosen deliberately: the handoff happens
   * in darkness, and the darkness has to arrive *before* the handoff, not
   * together with it. 0.96 leaves the last few percent of the pin sitting on
   * pure black. The start is pushed out to 0.9: before that the Policy Lock
   * has to open, deliver its light pulse and begin the answering move,
   * otherwise the curtain would hide the climax.
   *
   * Moving the end alone would not have been enough, though: the curtain ran
   * on smoothed progress and would lag the scroll at any headroom. So it is
   * taken off the common clock and driven by raw scroll — see `Hero.tsx`.
   */
  exit: { from: 0.94, to: 0.985 },

  /**
   * The copy flying past the camera.
   *
   * The copy is a fronto-parallel plane in front of the stone, and the camera
   * passes through it. For such a plane, perspective yields exactly a uniform
   * magnification about the vanishing point, so the "fly-past" here is not an
   * imitation: it is what a perspective projection turns depth motion into,
   * and nothing beyond that needs adding.
   *
   * `perspective` is the distance to the projection plane in frame heights.
   * The number is not arbitrary: the scene runs at `fov = 30°`, which is
   * `1 / (2·tan 15°) ≈ 1.866`. That is, the markup and the scene share **one
   * lens**. Taking more or less would mean placing two frames shot on
   * different optics side by side, and the join would show.
   *
   * `travel` is the fraction of that distance the copy travels toward the
   * viewer. The magnification comes out as `1 / (1 − travel)`, so 0.715 gives
   * roughly three and a half. That is enough for the block to leave through
   * the bottom edge by its own divergence, without a single pixel of manual
   * translation.
   *
   * `originY` is the vertical vanishing point. Exactly the middle of the
   * frame, and that is not a compositional choice but a property of the
   * projection: lines parallel to the view axis converge at the principal
   * point, and the principal point is where the camera looks — the centre of
   * the frame by definition.
   *
   * `rise` is the camera's rise **above** the plane of the copy, in frame
   * heights.
   *
   * Without it there is no fly-past, and that is a measured fact rather than a
   * subtlety. Depth alone gives magnification about the vanishing point, and a
   * copy block half a frame tall **straddles** that point: its lower half
   * diverges downward and its upper half upward, so it never leaves through
   * the bottom edge at any magnification. On a short viewport, where the block
   * takes two thirds of the height, instead of leaving it flooded the entire
   * frame and covered the stone.
   *
   * The rise is not a prop for the perspective but the second genuine
   * component of the same motion: in the scene the camera's aim travels from
   * zero up to the cube's centre, i.e. the camera rises. The copy lies below
   * it, and the camera's rise carries the copy downward whole, top edge
   * included. The magnitude comes from the requirement that the block leave
   * through the bottom edge on all three frame shapes, including the one where
   * it sits 0.2 of a height above the vanishing point.
   */
  fly: {
    perspective: 1.866,
    travel: 0.715,
    originY: 0.5,
    rise: 0.4,

    /**
     * A short fade of whatever is left at the very end of the fly-past.
     *
     * Insurance and nothing more: by the time it begins, the block is already
     * past the frame edge on every shape tested, and there is nothing left to
     * fade. The span is pushed almost to the very end deliberately — it used
     * to sit earlier, and then the fade turned from insurance into the primary
     * mechanism: the text managed to dissolve on screen, which is exactly what
     * it must not do.
     */
    fade: { from: 0.95, to: 1 },
  },

  /**
   * The bottom scrim departing.
   *
   * The scrim does not fly with the copy, and that is a difference in nature
   * rather than an economy: it is not an object in space but a darkening of
   * the frame. There is exactly one object flying past the camera — the copy;
   * a flying scrim would give away that the "three-dimensionality" here is
   * drawn rather than a consequence of the projection.
   *
   * It fades earlier than the copy: by the approach the stone has to be
   * standing on a clean background, and the scrim covers its lower third.
   *
   * It starts at zero rather than with the approach. The approach begins at
   * 0.1, and before that absolutely nothing would move: the first few percent
   * of the pin — which is a couple of dozen percent of the screen height —
   * would answer scroll with a still picture. The scrim can take that stretch
   * on, because it is the only thing here that is not an object in space: its
   * departure says nothing about where the camera stands, and so it is under
   * no obligation to be in phase with it.
   */
  floor: { from: 0, to: 0.35 },

  /**
   * Deceleration over the opening. The pause for the main event comes from
   * scroll density rather than from pin length: otherwise the camera approach
   * would take up two thirds in which nothing happens but getting closer.
   *
   * The peak is shifted toward the end of the span: the opening is interesting
   * on approach and at the moment the clearance is already wide, not in the
   * first frames of the parts separating.
   */
  accent: { from: 0.58, peak: 0.79, to: 0.9, gain: 2.6 },

  /**
   * The camera at the finale — the "canyon": a vertical fissure filling the
   * screen.
   *
   * Given as **an offset from the cube's centre** rather than as an absolute
   * point: the cube's rise in frame lives in `CUBE.centerY`, and an absolute
   * point would drift away from it at the first edit to the composition.
   *
   * The aim travels from zero to the cube's centre: at rest the stone hangs in
   * the upper part of the frame, and by the end of the approach it has to be
   * centred and filling it.
   *
   * The horizontal offset here is for the same reason as the turn: a camera
   * standing exactly opposite the fracture makes both of its walls
   * symmetrical, and the frame loses depth.
   *
   * A depth of 2.2 at `fov = 30°` gives a visible frame height of about 1.18
   * units, while the opened clearance is roughly 0.48 — a little under half
   * the frame. That is a canyon with both walls still visible; any closer and
   * the walls leave the edges, and what the lines fly out of is gone.
   */
  close: [0.62, 0.05, 2.2] as const,

  /** The final position at the surface: the vertical fissure becomes the frame. */
  handoffEye: [0.18, 0.04, 1.32] as const,
  handoffAim: [0, 0, 0] as const,

  /**
   * The turn to face front. The balancing is damped out by this point: two
   * systems rotating one object produce jitter where they meet.
   *
   * Not strictly zero. A cube turned exactly face-on to the camera loses its
   * volume: one face is visible and the stone reads as a flat texture rather
   * than a body. Fifteen degrees of vertical keep the side face and the lid in
   * frame — enough for the canyon walls to have thickness.
   */
  facePose: [-0.1, 0.26, 0.04] as const,

  /** How far the parts separate in the opening, in scene units. */
  open: 0.44,
  /** How much of the opening the guardrail takes back at the Policy Lock. */
  lockRecoil: 0.34,
  /** A brief compression of the stone at the point of impact, as a fraction of its scale. */
  lockImpactScale: 0.045,
  /** The camera's recoil from the impact, in scene units. */
  lockCameraKick: 0.18,

  /**
   * Defocus on the way out. Not for bokeh, but as a way of defocusing the
   * whole frame with an honest pass: the focus distance travels away from the
   * stone and `bokehScale` ramps from zero, so before the exit begins the pass
   * costs next to nothing.
   */
  bokeh: 7,
  focusAway: 12,

  /**
   * Chromatic aberration — a tenth of the usual. On a sharp frame it reads as
   * a defect, on a defocused one as optics, and the fade to darkness comes to
   * look like a lens rather than a CSS filter.
   */
  aberration: 0.0016,

  /** The curtain colour at the exit. */
  fadeTo: '#000000',
} as const

/** The stone. Its tone is corrected here rather than baked into the texture. */
export const STONE = {
  /**
   * The albedo multiplier. Not "make it darker" — the map is already dark —
   * but a correction of tone: take the green of moss out and carry the stone
   * to the cold side, where the blue from the fissures reads as a continuation
   * rather than as a foreign colour.
   */
  tint: '#e6ebf2',

  /** How much the texture is scaled relative to a scene unit. */
  triplanarScale: 2.1,

  /**
   * The three projections get different rotations and scales so their
   * boundaries do not coincide with the stone's own layering.
   */
  projectionSkew: [0, 0.37, 0.71],

  /**
   * The second overlay octave against tiling: a scale multiplier, an extra
   * rotation and a weight. The scale is deliberately not a round number —
   * otherwise the octaves would fall back onto one lattice and the pattern
   * would return.
   */
  detailScale: 0.413,
  detailSkew: 0.29,
  detailMix: 0.42,

  /**
   * The crack network from a photoscan (the blue channel of `orm.webp`; see
   * `public/textures/CREDITS.md` for the sources and how the channel was
   * extracted). Noise does not produce such a network: it draws grooves,
   * whereas a fracture has junctions, branches and dead ends, and it is those
   * that read as real.
   *
   * `crackDarken` — how much darker the cavity is than the rock;
   * `crackRelief` — how steeply the normal tips into it; `crackRoughen` — the
   * added roughness: a fresh break inside a crack is not polished by weather.
   */

  /**
   * The network's scale relative to the rock's. Below one means the network is
   * coarser than the grain, and that is mandatory: the stone's grain is
   * measured in millimetres and its fractures in centimetres. At a shared
   * scale the network comes out speckled.
   */
  crackScale: 0.28,

  crackDarken: 0.62,
  crackRelief: 1.5,
  crackRoughen: 0.12,

  normalStrength: 1.45,
  roughness: 0.8,
  aoIntensity: 1.0,
} as const

export type TierSettings = {
  /** Quads per face along one axis. */
  subdivision: number
  /** Upper bound on devicePixelRatio. */
  maxDpr: number
  bloom: boolean
  /** Defocus and aberration at the end of the pin. */
  finalOptics: boolean
  /**
   * MSAA samples on the composer's buffer.
   *
   * This is the most expensive knob in the whole scene, and it is expensive
   * non-linearly: the composer's buffer is half-precision, so every sample is
   * eight bytes per pixel that have to be written and then read back at
   * resolve. `EffectComposer` defaults to eight, and on a 3840×2160 frame that
   * is half a gigabyte of traffic every frame — several milliseconds before
   * anything is drawn at all.
   *
   * Four is enough: what gets antialiased here is the cube's silhouette and
   * the edges of the chips, and those are long smooth boundaries where the
   * difference between 4× and 8× does not resolve even under frame-by-frame
   * comparison.
   */
  multisampling: number
  /**
   * Anisotropy of the stone maps.
   *
   * It multiplies the number of taps: the triplanar setup takes up to
   * twenty-one per pixel, and an anisotropy of 8 turns that into a hundred and
   * fifty texel fetches. The stone stands almost face-on to the camera and
   * there are no grazing angles in frame — there is almost nowhere for
   * anisotropy to do work here, while it is paid for everywhere.
   */
  anisotropy: number
  /** The fraction of the resolution the defocus is computed at. */
  dofResolution: number
}

/**
 * Triangles: 4 parts × 6 faces × subdivision² × 2.
 * 64 → 197k · 48 → 111k · 32 → 49k.
 *
 * Faces do not share vertices with one another — the cube has to have sharp
 * edges, and a chip only reads as a facet with a normal of its own. Duplicated
 * vertices along face boundaries cost on the order of a percent in return.
 *
 * `maxDpr` is the second strongest knob after MSAA, and it is quadratic:
 * 2.0 → 1.5 removes 44% of all fragments at once, across every pass. On a
 * retina display 1.5 reads sharp, because the antialiasing is done by the
 * composer anyway, not by pixel density.
 */
export const TIER_SETTINGS: Record<Tier, TierSettings> = {
  full: {
    subdivision: 64,
    maxDpr: 1.5,
    bloom: true,
    finalOptics: true,
    multisampling: 4,
    anisotropy: 4,
    dofResolution: 0.4,
  },
  reduced: {
    subdivision: 48,
    maxDpr: 1.25,
    bloom: true,
    finalOptics: false,
    multisampling: 2,
    anisotropy: 2,
    dofResolution: 0.4,
  },
  low: {
    subdivision: 32,
    maxDpr: 1,
    bloom: true,
    finalOptics: false,
    multisampling: 2,
    anisotropy: 1,
    dofResolution: 0.4,
  },
  /**
   * The last resort. `bloom: false` drops the expensive pass, but the composer
   * itself stays mounted on every tier — ACES tone mapping lives only in that
   * chain, and a tier without it would render a visibly different picture. See
   * `PostProcessing.tsx`.
   */
  still: {
    subdivision: 32,
    maxDpr: 1,
    bloom: false,
    finalOptics: false,
    multisampling: 0,
    anisotropy: 1,
    dofResolution: 0.4,
  },
}

/**
 * The quality ladder, driven by frame time.
 *
 * The thresholds are not taste but a consequence of having to measure in front
 * of the visitor. A warm-up is mandatory: the first frames contain shader
 * compilation, map loading and the construction of the environment's PMREM,
 * and judged by those every machine looks weak. The median rather than the
 * mean: a single 200 ms frame is a garbage collection or a tab switch, and it
 * moves the mean but not the median.
 *
 * A tier drops on exceeding a budget of 20 ms rather than 16.7: right on the
 * sixty-hertz boundary the ladder would chatter, and it is one-way and has no
 * right to be wrong.
 */
export const LADDER = {
  /** Warm-up frames, which do not count at all. */
  warmup: 90,
  /** Length of the measurement window in frames. */
  window: 60,
  /** The median frame above which the tier drops, milliseconds. */
  budget: 20,
  /** Frames of quiet after a drop: the new tier needs to settle. */
  cooldown: 120,
  /**
   * A frame longer than this is not a frame but a stall.
   *
   * The threshold is deliberately high, and a low one here would not be
   * caution but a plain mistake: it would reject not stalls but the real
   * frames of weak machines. At a threshold of 100 ms, anything slower than
   * ten frames per second never enters the window **even once**, the window
   * never closes, and the ladder stays silent on exactly the machine it was
   * written for. Verified: on a software rasteriser at 833 ms per frame no
   * descent happened.
   *
   * Protection from a single spike comes from the median, not from this
   * threshold. A hidden tab does not produce a run of long frames at all: rAF
   * is not called there, and on return a single gap arrives, which does not
   * move the median. All the threshold has to reject is that gap, and it is
   * measured in seconds.
   */
  outlier: 1000,

  /**
   * The same three stretches in milliseconds; a stretch closes on whichever of
   * its two limits arrives first.
   *
   * Frames alone are not enough, and the error runs exactly counter to what is
   * needed: the weaker the machine, the longer it takes to accumulate ninety
   * warm-up frames and sixty window frames. At fifteen frames per second the
   * first drop would happen in the tenth second, at five in the thirtieth.
   * That is, the visitor the ladder exists for waits longest of all.
   *
   * The converse limit — time only — will not do either: on a fast machine a
   * second and a half is ninety frames, and on a very fast one two hundred and
   * forty, so the window would measure for longer than necessary and gain
   * nothing.
   */
  warmupMs: 3000,
  windowMs: 1500,
  cooldownMs: 2000,

  /**
   * Below this many frames the median means nothing: of five numbers it is
   * merely the third largest. The time limit cannot close the window earlier.
   */
  minSamples: 10,
} as const

/**
 * Scene time under `prefers-reduced-motion`.
 *
 * Not zero: at phase zero all three sways and the brightness breathing sit at
 * their starting point simultaneously, and the stone stands in a pose that
 * never occurs in motion. The number is incommensurable with every `BALANCE`
 * period, so the pose is arbitrary in the same sense that any freeze frame is.
 */
export const STILL_TIME = 4.3
