# Landing design notes

What is deliberate about the marketing surface, and why. This file covers the
decisions a reader cannot recover from the code alone. Anything mechanical —
the palette, the type scale, the spacing steps — lives as custom properties at
the top of `src/index.css` and is authoritative there; do not restate values
here, they will drift.

## Type

Inter carries prose and headings; JetBrains Mono carries anything the visitor
could paste into a shell. Both are self-hosted variable faces under
`public/fonts` and preloaded from `index.html`, so the swap lands before first
paint instead of reflowing the hero. Weights stay in a low 400–590 band and
tracking stays tight — the page gets its emphasis from scale and contrast, not
from bold.

## Motion

### Hero scroll sequence

**Role:** the one authored moment on the marketing surface.

A pre-rendered shot — a machine hand holding a suspended, fractured plate of
light, the tethers running off its wrist drawing taut and releasing — scrubbed
by scroll through a Canvas 2D blit (`src/components/ui/FrameSequence.jsx`),
driven by a single GSAP ScrollTrigger timeline pinned for 160vh.

**The render is a single directed reach, and only its first 119 frames are
used.** A machine hand rises out of the dark toward a suspended, fractured pane
of light, opens into its bloom, and is at full extension by ~114 without
touching it. 161 frames ship, but past the arrival the hand recoils and settles,
and 130–161 is a near-static tail of the same held pose. Scrubbed, that tail
would be a third of the pin in which the picture does not change and the one
thing that did happen quietly comes undone, so `range` stops the span at 119 —
a few frames past the arrival, not enough to start giving it back. Scroll
position *is* frame position; scrolling up rewinds, which is the visitor undoing
their own scroll.

**The span is not uniform, and it is not scrubbed as if it were.** Frames 1–90
are travel: the hand crossing the dark. The arrival is the only *event* in it,
and it is the product's argument in one image — the hand is not stopped, it is
held just short. It gets the emphasis.

The emphasis is authored as a **scroll density**, not as a segmented map. What
the code defines is how much scroll each part of the span costs:
`1 + gain × bump`, flat to frame 90 and humped from there to the end of the
span, integrated once at module load into a 512-step table that is read
backwards to turn pin progress into frame position. At a gain of 3 the accent
span takes ~40% of the pin against the ~25% its frame count would earn evenly,
and the shot runs at quarter speed through the arrival.

The hump is a raised cosine, and the shape is the point. Two or three linear
segments meeting at frames 90 and 112 would be continuous in *position* but not
in *speed*: at each joint the frames-per-pixel rate changes instantly and the
shot visibly drops a gear going in and picks it back up coming out. A raised
cosine has zero slope at all three of its joints, so the slowdown arrives, peaks
and leaves at the ambient rate. The visitor should feel the shot get heavy as
the hand nears the pane, not catch the moment it does.

The hump's peak is placed at frame 112 rather than centred, because the footage
is asymmetric: the approach is 22 frames of travel worth dwelling on, and the
seven frames after the arrival are all but identical to it, so the tail is cheap
and reads as the pin releasing rather than as the shot speeding up. A centred
bump would spend the dwell halfway up the reach and hit full ambient speed at
exactly the frame the whole shot is for.

The curve lives in the map, aimed at a named span of footage, rather than on the
tween, where it would silently re-time the text beats as well. The map is
monotonic, so direction-of-travel behaviour is unaffected.

**Smoothness is three mechanisms, and none substitutes for another.** They are
worth keeping distinct because each fixes a case the others cannot see.

*Sub-frame interpolation* fixes a slow, deliberate scroll. The renderer draws a
fractional position, not a frame index: the two frames the position falls
between are composited at the fractional weight. Without it the shot advances a
whole frame at a time — a hundred-odd discrete steps — however continuously the
timeline is driven underneath, because the scrub is continuous and the picture
is not, and what the eye reports is the picture.

*Scrub lag* (0.5s) fixes a discrete input. A wheel notch is a fifty-pixel jump
with nothing in between, so a position-mapped scrub has no intermediate values
to draw; easing the timeline toward the scroll position gives the canvas a run
of ticker frames to be driven through instead. Half a second is the ceiling —
the lag is a debt the shot repays before the pin releases.

*Pin length* (150vh) is how much scroll the shot is worth: three or four
trackpad flicks, enough that the reach is travelled through rather than
overshot, and enough that the text beats land as beats. It buys no smoothness on
its own. At a 900px viewport it is 1350px over a 119-frame span, which leaves
the approach at roughly eight pixels a frame and the arrival at about
twenty-five. Shorter and the reach reads as a hurry to get to the good part;
longer and the travel before frame 90 outstays its interest.

The backing store is capped at 2.7 megapixels, scaled down proportionally so the
fit maths never sees it. Uncapped, the cost of a frame is set by the visitor's
monitor — a 2560px window at DPR 2 asks to resample the source into 14.7
megapixels, twice per frame now that frames blend — and there is nothing to buy
with them, since the desktop render is only 1920px wide.

Nothing in the scrubbed timeline animates a filter. The one-shot entrance can
afford a blur; the scrubbed beats cannot, because that is a filter re-resolved
over a text block on every frame the canvas is also compositing two
full-viewport blits on. Transform and opacity carry the same accretion for free.
Scrubbed arrivals also use `power2.out`, not `expo.out`: under a scrub the
visitor is the clock, so an exponential curve inverts and spends nine tenths of
its movement in the first tenth of the scroll — the beat snaps in over a few
pixels and then crawls.

Frames are held as `<img>` elements decoded ahead of paint, never as
`ImageBitmap`, so the browser can evict decoded surfaces under memory pressure.
The download order is coarse to fine — every 8th frame, then 4th, 2nd, all —
and viewer-biased within each pass: a visitor reaches the end of the shot long
before the whole span has landed, so the first pass buys the *whole* gesture at
low frame rate rather than a fraction of it at full. Frames that have not landed
widen the blended pair instead of breaking it, so the gap plays as a dissolve —
the sequence degrades in smoothness, never into a step or a blank canvas.

Text accretes over the pin in three beats — headline and actions, then the
subhead, then the install snippet — rather than swapping: nothing already read
is taken away, and the pin releases on the complete composition. Both later
beats are in by 0.48, which the accent map makes exactly the right place for
them: frame 90 lands at 55% of the pin, so the copy finishes arriving just
before the reach starts to slow.

The scroll rail leaves through 0.74–0.9, inside the accent, and that is
deliberate. A progress meter is the one thing that can flatten the emphasis — it
reports that the scroll is still moving at its usual rate while the picture
deliberately is not, inviting the visitor to read the slowdown as the page
lagging. It is gone by frame 112, so the shot has the frame to itself through
the arrival.

Under `prefers-reduced-motion: reduce` the pin is never created and the hero
renders at frame 114 — the hand at full extension, fingers open inside the
pane's bloom and not touching it. Neither end of the reach is a composition, so
the still is the frame the accent exists to dwell on.

**The render carries no headroom, so the composition supplies it.** The pane's
top edge sits ~4% down the 16:9 field, which leaves the glass pinned against the
nav; the shot is dropped 10% of frame height on desktop and 12% on the phone's
band. That is the most it takes — there is almost no image above the pane, so
past a small nudge the exposed band stops reading as air and starts reading as a
bar, and zooming to the same drop would need ~3.4× and throw the hand out of
frame. The exposed band is masked with the substrate at exactly `shiftY`, both
sized from the same constants so they cannot drift apart, fading 6% past it
because the shot's top row is not quite the substrate's black.

The desktop scrim is heavier than a shot with a contained highlight would need:
this render throws a wide, soft column of bloom down the middle of the field, so
the grade manufactures the headline's contrast rather than merely guaranteeing
it. It holds most of its weight to 68% of the width and is clear by 88%; the
pane sits past 90% and is never touched.

Two frame sets ship, both cut from the same render, which is why positions are
expressed as fractions of the render rather than as frame numbers:
`public/frames` at 1920w for `min-width: 768px`, and `public/frames-sm` at 960w
every second frame for phones and `save-data`. The phone gets the same shot at
half the temporal resolution and roughly a fifth of the bytes; because fractions
resolve per set, the trim and the accent land on the same moments at either
breakpoint.
