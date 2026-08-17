# Landing design notes

What is deliberate about the marketing surface, and why. This file covers the
decisions a reader cannot recover from the code alone. Anything mechanical —
the palette, the type scale, the spacing steps — lives as custom properties at
the top of `src/app/globals.css` and is authoritative there; every number in
the hero scene lives in `src/lib/scene/config.ts` and is authoritative there.
Do not restate values in this file, they will drift.

This is the only design document for the surface. The hero was built against a
written brief, but a pre-code brief goes stale the moment the code disagrees
with it, and keeping both invites a reader to trust the wrong one — so the brief
is gone and everything worth keeping from it is stated here, as an account of
what the code actually does.

## Type

Inter carries prose and headings; JetBrains Mono carries anything the visitor
could paste into a shell; Shadows Into Light is the brand-signature accent, and
appears exactly twice — the hero eyebrow and the footer wordmark. All three are
self-hosted under `public/fonts`. Inter and JetBrains Mono carry the hero, so
both are preloaded from `src/app/layout.tsx` and the swap lands before first
paint instead of reflowing it; Shadows is not preloaded, being a single short
label off the critical path.

Headings take Inter Bold and body copy runs at 400. The two-tone emphasis
inside a heading — a dimmer opening clause resolving into full Cloud White — is
carried by colour rather than by a third weight, and tracking stays tight: the
page gets its emphasis from scale and contrast.

## Space

Two steps of vertical rhythm and one horizontal gutter, all three custom
properties in `src/app/globals.css` — `--spacing-section`,
`--spacing-section-lg`, `--spacing-gutter`. Every top-level section pads itself
with the pair and nothing else, so the gap between any two of them is the same
gap, and a divider laid on that boundary is centred in it without being told to
be.

They exist because the alternative was measured: with each section carrying
its own number the inter-section gaps ran 304, 272, 240, 232 and 224px down
the page — a 1.36× spread, monotonically decreasing, which reads as the page
losing interest in itself on the way down.

The gutter is a `clamp` rather than a set of breakpoints, because the content
column is fluid from the phone to about 950px and a step would only move the
seam. It leaves the phone where it was and stops growing once the 1200px
max-width is what governs the column anyway.

Two names, not a scale: section rhythm is the thing that drifts across a
page this long, and component rhythm is already served by the numeric
utilities.

One section does not centre its content in a viewport-tall box, and none of
them should. `min-h-svh` on a padded section is two instructions for one
job — the padding is already inside the min-height — and all the min-height
adds is emptiness that scales with the visitor's window. The claim section
carried 616px of it on a 1024px-tall tablet before this was removed.

## Motion

### The hero: a fractured stone under a guardrail

**Role:** the one authored moment on the marketing surface.

A cube of dark rock, split into four quarters along the planes X = 0 and
Y = 0, hanging slightly above the centre of a lit blue field with light in its
fractures. It is a real-time WebGL scene — react-three-fiber, in
`src/components/hero/` — pinned for 200vh and scrubbed by scroll.

It replaced a pre-rendered 161-frame canvas sequence, and the reason it did is
worth recording, because the sequence was not obviously worse: it was 8.7 MB of
frames that could not answer the cursor, could not be relit, and had to be
re-rendered offline for any change at all. The static export dropped from 13 MB
to 3.8 MB with the swap. What the scene buys beyond the bytes is that the
central gesture is now a consequence of the model rather than a picture of one —
see the glow, below.

#### One number between scroll and scene

The pin publishes a single value, its progress in 0..1, into a ref. Everything
downstream reads that ref: the camera, the cube, the post-processing chain and
the copy. There is deliberately no second channel and no per-object timeline
that could drift out of phase with the first.

Pin progress is not scene progress. Between them sits a **scroll density map**
(`src/lib/scene/progress.ts`): what is declared is not a partition into
segments but the price of scroll — how many pixels each part of the scene
costs. `1 + gain × bump` is integrated once into a 512-step table and read
backwards. The opening of the fracture is the event the whole scene is for, and
the map buys it a dwell without lengthening the pin; without it the camera
approach would occupy two thirds of the scroll in which nothing happens but
getting closer.

The bump is a raised cosine, and the shape matters more than the gain. Two or
three linear segments meeting at the span's ends would be continuous in
*position* but not in *speed*: at each joint the rate changes instantly and the
scene visibly drops a gear going into the slowdown and picks it back up coming
out. A raised cosine has zero slope at all three of its nodes. The map is
monotonic, so scrolling back is playback in reverse and nothing needs a special
case for it.

Because the map is where the emphasis lives, the spans in `SCROLL` are declared
in *scene* progress rather than pin progress — 0.6–0.85 of the scene occupies
appreciably more than a quarter of the pin.

#### The beats

Camera approach, the copy flying past, the fracture opening, the Policy Lock,
the handoff to black. Two of those deserve their reasoning here.

**The copy does not fade — the camera drives through it.** It is a
fronto-parallel plane in front of the stone, and its motion is driven by the
*same span and the same curve* as the camera approach, not by a duration of its
own. A duration of its own would mean the text and the stone live in two
different spaces that happen to be overlaid, and that is exactly what used to
read as "the text disappeared": it faded on its own clock while the camera was
still only starting to move. The magnification is done entirely by perspective,
and the CSS `perspective` value is derived from the scene's own 30° fov, so the
markup and the scene share one lens. Depth alone is not enough and this was
measured, not assumed: a copy block half a frame tall straddles the vanishing
point and never leaves through the bottom edge at any magnification, so the
camera's rise — which the scene performs anyway, moving its aim up to the
cube's centre — carries the block out whole.

**The curtain is the one thing driven by raw scroll.** Everything else runs on
the smoothed scrub, which is correct: a wheel notch is a fifty-pixel jump with
no intermediate values and the scene needs frames to fill in between. But
smoothing is lag, and the pin releases on exactly its last pixel. Measured, a
curtain on the common clock reached full black at 103.3% of the pin — sixty
pixels after the page had already started moving, so the visitor watched a lit
canyon at a quarter opacity turn into a flat sheet and slide upward. The
curtain has the right to leave the common clock for the same reason the bottom
scrim does: it is not an object in space but a darkening of the frame. Objects
must not drift apart from one another; a darkening has nothing to drift from.

#### Why the parts mate

Each quarter is displaced by a vector field evaluated at the vertex's position
**in the coordinates of the whole cube, before the cut**. The displacement
depends on nothing else — not on which part the vertex belongs to, not on the
face normal. So two coincident vertices of neighbouring parts get the same
displacement and stay coincident: the mating surface is not fitted, it is one
surface computed twice. This is also why the displacement cannot follow the
normal, and why no part may ever rotate on its own.

The lift direction is the pure diagonal, which is the only direction carrying a
part away from *both* cut planes at once. The brief asked for the parts to
rise, so the diagonal is mixed with +Y — and that mixing has a hard ceiling at
`1/(1+√2)`, past which a lower part starts sliding *along* its cut plane and
the jags pierce each other. The configured bias sits inside the limit with
margin, and a test asserts it.

#### The glow is a consequence, not an animation

Light lives on the fracture surfaces and grows toward the cube's centre, so
what shows at rest is a lit lip rather than a neon tube. When a part moves
away it exposes a deeper — and therefore brighter — band of surface to the
camera, and the intensification comes for free. There is no separate flash to
animate, which is precisely why light and motion cannot fall out of step. Only
the page's two accent colours appear in it: Electric Blue as the body, Cyan
Neon reserved for the hottest places.

The crack network across the rock comes from a photoscan channel, not from
noise. Noise draws grooves; a fracture has junctions, branches and dead ends,
and it is that statistic the eye recognises as real. It also has to come from a
texture rather than be computed: a hash lattice aliases at any distance where a
cell falls below a pixel — sparkle in stills, shimmer in motion — and no mipmap
saves something that is never sampled. Sources and licences are in
`public/textures/CREDITS.md`.

#### Cost

Three knobs dominate, and they are ranked. MSAA on the composer's buffer is
first and is non-linear — the buffer is half-precision, so every sample is
eight bytes per pixel written and read back at resolve; the default of eight
samples is half a gigabyte of traffic per frame at 4K, before a single fragment
is shaded. `maxDpr` is second and is quadratic. Subdivision is third and is the
cheapest of the three to give up.

Quality descends a **one-way** ladder measured in frame time
(`src/lib/scene/ladder.ts`): down only, never back up. A two-way ladder
oscillates and the visitor watches the picture breathe quality at them, which
is worse than a steady low tier. The measurement uses a median rather than a
mean, warms up first, and gives every stretch two limits — in frames *and* in
milliseconds — closing on whichever arrives first. Frames alone would stretch
the ladder further the weaker the machine, so the visitor it exists for would
wait longest of all; verified on a software rasteriser at 833 ms per frame,
where an earlier outlier threshold left it silent forever.

The composer is never dropped, on any tier. ACES tone mapping lives only in
that chain and the renderer is deliberately `NoToneMapping`, so a tier without
it would ship a visibly different, untone-mapped picture to exactly the
weakest machine. The lowest tier drops Bloom instead, which is the expensive
pass.

The canvas stops drawing entirely once the hero leaves the screen — with a
200vh pin the visitor spends most of the visit below it, so the scene stands
switched off longer than it stands on — and resumes 20% of a viewport early, so
the one frame that switching on costs does not land on the visible edge.

Phones get a 512² normal map instead of the 1024² desktop one, chosen by the
same media query in both the preload and the loader; letting those diverge
would mean preloading one file and fetching another. The preloads carry
`crossOrigin`, which is not decoration: Three's ImageLoader requests as
`anonymous`, and a preload without the attribute is a different CORS mode, so
the browser refuses the warm entry and fetches every map twice.

#### Reduced motion

Under `prefers-reduced-motion: reduce` the pin is not created at all, rather
than created with zero durations: a pin is itself page motion. The scene stays
at full quality and renders one still frame on demand. Scene time is frozen at
a value incommensurable with all three sway periods — not at zero, where every
oscillator sits at its starting point simultaneously and the stone stands in a
pose that never occurs in motion.

#### What is not in the scene

The blue field the stone hangs in is a CSS gradient in the markup, not scene
geometry: it must not cost a frame. It darkens from the outside in and reaches
full black at 96% — inside the frame rather than past its edge — which is what
makes the join with the section below seamless, since that section stands on
pure `#000`.

There is deliberately no poster image behind the canvas. A baked still would
drift away from the material at the first edit, and the discrepancy would
surface a month later on somebody's phone. What the visitor sees while the GPU
assembles the first frame is a labelled loading state, and it only leaves after
a frame in which the finished stone could actually have been painted.

### Elsewhere on the page

Scrubbed arrivals use `power2.out`, not `expo.out`: under a scrub the visitor
is the clock, so an exponential curve inverts and spends nine tenths of its
movement in the first tenth of the scroll — the beat snaps in over a few pixels
and then crawls. Nothing in a scrubbed timeline animates a filter; transform
and opacity carry the same accretion for free.
