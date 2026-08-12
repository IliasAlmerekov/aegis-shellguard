import { useEffect, useRef, useState } from 'react'
import { gsap, ScrollTrigger, useGSAP } from '../../lib/gsap'
import { FrameSequence } from '../ui/FrameSequence'

/* A phone's URL bar collapsing counts as a resize, and a resize recalculates
   every pin on the page. Mid-scroll that lands as a jump: the pin's start
   moves under the visitor while the shot is playing. The viewport did not
   really change — `h-svh` is already the smallest height — so the refresh has
   nothing to fix and everything to disturb. */
ScrollTrigger.config({ ignoreMobileResize: true })

const INSTALL_CMD = 'npm i -g @iliasalmerekov/aegis'

const SHOT_LABEL =
  'A machine hand rises out of the dark toward a suspended, fractured pane of light and stops just short of touching it.'

/* ── The shot, its usable span, and where it is emphasised ────────────
   161 frames on disk, but the gesture is not 161 frames long. The hand rises
   out of the dark, opens into the plate's bloom, and is at full extension by
   ~114. After that it recoils and settles, and 130–161 is a near-static tail
   of the same held pose. Scrubbed, that tail is a third of the pin in which
   the picture does not change and the one thing that did happen quietly comes
   undone.

   So the span stops at 119 — a few frames past the arrival, enough for the
   reach to land and not enough to start giving it back. `SHOT_RANGE` is in
   fractions of the render because that is what `FrameSequence` resolves
   against whichever frame set the device gets; everything else on this page is
   in source frame numbers, through `atFrame`. */
const SHOT_FRAMES = 161
const SPAN_TO = 119

const SHOT_RANGE = [0, (SPAN_TO - 1) / (SHOT_FRAMES - 1)]

/* A source frame number as a position in the span — the unit `seq.show` and
   the accent map both work in. */
function atFrame(n) {
  return (n - 1) / (SPAN_TO - 1)
}

/* The arrival is the one *event* in the footage rather than travel, and it
   happens to be the product's argument in one image: the hand is not stopped,
   it is held just short. It gets the emphasis — a slowdown that builds through
   the approach from frame 90, is heaviest where the fingers sit in the plate's
   bloom at 112, and is spent by the end of the span. */
const ACCENT_FROM = 90
const ACCENT_PEAK = 112
const ACCENT_TO = SPAN_TO

/* Peak slowdown, as a multiple of the baseline scroll cost per frame. At 3 the
   shot runs at a quarter speed through the arrival — a real dwell, not a
   hesitation — and the accent span takes about 40% of the pin against the 25%
   its frame count would earn evenly.

   Higher stalls it: past ~5 the visitor scrolls and the picture stops, which
   reads as the page having hung rather than as emphasis. */
const ACCENT_GAIN = 3

const ACCENT_A = atFrame(ACCENT_FROM)
const ACCENT_P = atFrame(ACCENT_PEAK)
const ACCENT_B = atFrame(ACCENT_TO)

/* A raised cosine over the accent span — 0 at both edges, 1 at the peak — with
   the peak placed rather than centred.

   The smooth shape is the whole point, and it is why this is not two or three
   linear segments meeting at frames 90 and 112. A segmented map is continuous
   in *position* but not in *speed* — at each joint the frames-per-pixel rate
   changes instantly, and the shot visibly drops a gear on the way into the
   arrival and picks it back up on the way out. It is the exact artefact
   "smooth emphasis" rules out. A raised cosine has zero slope at every one of
   its three joints, so the slowdown arrives, peaks and leaves at the ambient
   rate and there is nothing to catch.

   Asymmetric because the footage is: the approach is 22 frames of travel worth
   dwelling on, and the seven frames after the arrival are all but identical to
   it, so the tail is cheap and reads as the pin releasing rather than as the
   shot speeding up. A centred bump would spend the dwell halfway up the reach
   and hit full ambient speed exactly at the frame the whole shot is for. */
function accentBump(u) {
  if (u <= ACCENT_A || u >= ACCENT_B) return 0
  const half = u < ACCENT_P ? (u - ACCENT_A) / (ACCENT_P - ACCENT_A) : (ACCENT_B - u) / (ACCENT_B - ACCENT_P)
  return 0.5 * (1 - Math.cos(Math.PI * half))
}

/* Pin progress → position in the render, as a lookup table.

   Built the way round that keeps the maths honest. What is actually being
   authored is a *density*: how much scroll each part of the render costs,
   which is `1 + gain × bump` — flat everywhere, humped over the tightening.
   Integrating that gives pin-progress-as-a-function-of-frame, which is the
   inverse of what the timeline needs, so it is sampled once at module load
   and read backwards.

   512 steps. The underlying curve is smooth, so the table's linear segments
   are a 1/512 approximation of it — far below a frame, and far below the
   pixel the position is eventually drawn at. Binary searched on the way out:
   nine comparisons, once per ticker frame, against a Float64Array. */
const ACCENT_STEPS = 512

const ACCENT_MAP = (() => {
  const cum = new Float64Array(ACCENT_STEPS + 1)
  for (let i = 1; i <= ACCENT_STEPS; i += 1) {
    const u0 = (i - 1) / ACCENT_STEPS
    const u1 = i / ACCENT_STEPS
    const d0 = 1 + ACCENT_GAIN * accentBump(u0)
    const d1 = 1 + ACCENT_GAIN * accentBump(u1)
    cum[i] = cum[i - 1] + (d0 + d1) / 2
  }
  const total = cum[ACCENT_STEPS]
  for (let i = 0; i <= ACCENT_STEPS; i += 1) cum[i] /= total
  return cum
})()

function shotPosition(p) {
  const clamped = p <= 0 ? 0 : p >= 1 ? 1 : p
  let lo = 0
  let hi = ACCENT_STEPS
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1
    if (ACCENT_MAP[mid] <= clamped) lo = mid
    else hi = mid
  }
  const span = ACCENT_MAP[hi] - ACCENT_MAP[lo]
  const t = span > 0 ? (clamped - ACCENT_MAP[lo]) / span : 0
  return (lo + t) / ACCENT_STEPS
}

/* How far the pin runs, as a share of the viewport, and how far the timeline
   is allowed to trail the scroll position.

   Three mechanisms decide whether this reads as footage, and they answer
   different problems — none of them substitutes for another:

   `PIN_LENGTH` is how much scroll the shot is worth. 150% for a 119-frame
   span: at a 900px viewport that is 1350px, and the accent's ~40% share leaves
   the approach at roughly eight pixels a frame and the arrival at about
   twenty-five. Shorter and the reach reads as a hurry to get to the good part;
   longer and the travel before frame 90 — which is genuinely just travel —
   outstays its interest. Neither number is what buys smoothness.

   `SCRUB` is what makes a *discrete* input continuous. A wheel notch is a
   ~50px jump with nothing in between, so a position-mapped scrub has no
   intermediate values to draw and steps by six frames per notch however dense
   the sequence is. GSAP eases the timeline toward the scroll position over
   SCRUB seconds instead, which gives the canvas a run of ticker frames to be
   driven through. Half a second is the ceiling: the lag is a debt the shot has
   to repay before the pin releases, and a longer one leaves a visitor who
   flicks straight through watching the hero leave while the shot is still
   catching up.

   Sub-frame interpolation in `FrameSequence` is what makes a *continuous*
   input continuous — it is the one that fixes a slow, deliberate scroll, where
   there is no lag debt to smooth anything and the shot would otherwise advance
   one whole frame at a time. See `paint` there. */
const PIN_LENGTH = '+=150%'
const SCRUB = 0.5

/* Where the shot sits for `prefers-reduced-motion`. Frame 114: the hand at
   full extension, fingers open inside the plate's bloom and not touching it.
   It is the frame the accent exists to dwell on, which makes it the right
   still for a visitor who will only ever see one. */
const SHOT_STILL = atFrame(114)

/* The shot's subject — hand, arm, plate — occupies roughly the middle of the
   right third of a 16:9 frame, so cover-fit has to be told which part of the
   width to keep; centred, it crops the hand away.

   A phone gets the shot as a full-bleed band rather than a full-height
   cover: cover-fitting 16:9 into a 9:19.5 viewport is a 3.8× horizontal
   overscan, which leaves the plate filling the screen and the hand outside
   it. The band's own aspect is close enough to the subject's that the whole
   vertical gesture — hand rising to plate — stays in frame. */
const FOCUS_DESKTOP = 0.6
const FOCUS_MOBILE = 0.88

/* The shot's ground. Every gradient in this section grades to it and the
   canvas clears to it, so it is one constant rather than a literal repeated
   six times. It mirrors `--color-night-void`: a darkened Night Black, chosen
   to sit inside the range the footage's own corners fade to (#00050a …
   #040d13) so the scrim has no edge to give away. */
const SUBSTRATE = '#04090e'
/* The one colour on this page that cannot be a token: it is handed to
   `ctx.fillStyle`, and a canvas context resolves no custom properties. Kept
   in step with `--color-night-void` by hand — if that value moves, this one
   moves with it. */

/* This render carries no headroom of its own: the pane's top edge sits about
   4% down the 16:9 field, which leaves the glass pinned against the nav.
   Dropping the whole window gives it air; what leaves the bottom of the frame
   is the tail of the cabling, already fading to black.

   How far it can drop is capped by the source, not by taste. There is almost
   no image above the pane, so the exposed band is substrate and nothing else,
   and past a small nudge it stops reading as air and starts reading as a bar
   between the nav and the shot. Zooming instead of shifting does not buy the
   drop either — anchored to the top edge it takes roughly 3.4× to move the
   pane the same distance, which throws the hand and most of the arm out of
   frame. These values are the most this composition takes. */
const SHOT_ZOOM = 1
const SHIFT_DESKTOP = 0.1
const SHIFT_MOBILE = 0.12

/* The desktop scrim's grade, as [position %, opacity] pairs of the substrate.
   It holds most of its weight out to 68% and is clear by 88% — the pane itself
   sits past 90% and is never touched. */
const SCRIM_GRADE = [
  [0, 1],
  [34, 0.96],
  [52, 0.82],
  [68, 0.34],
  [80, 0.08],
  [88, 0],
]

const SCRIM_STOPS = SCRIM_GRADE.map(
  ([at, alpha]) =>
    `color-mix(in srgb, ${SUBSTRATE} ${alpha * 100}%, transparent) ${at}%`
).join(', ')

/* How far past the exposed band the seam mask keeps fading, as a share of
   frame height. The shot's top row is not quite the substrate's black, so the
   join would otherwise read as a hairline; this clears it without reaching the
   pane. */
const SEAM_FADE = 0.06

function useMinWidth(query) {
  const [matches, setMatches] = useState(() =>
    typeof window === 'undefined' ? true : window.matchMedia(query).matches
  )
  useEffect(() => {
    const mql = window.matchMedia(query)
    const onChange = () => setMatches(mql.matches)
    onChange()
    mql.addEventListener('change', onChange)
    return () => mql.removeEventListener('change', onChange)
  }, [query])
  return matches
}

export function Hero() {
  const rootRef = useRef(null)
  const pinRef = useRef(null)
  const seqRef = useRef(null)
  const [copied, setCopied] = useState(false)
  const isDesktop = useMinWidth('(min-width: 768px)')

  const handleCopy = async () => {
    try {
      await navigator.clipboard?.writeText(INSTALL_CMD)
      setCopied(true)
      setTimeout(() => setCopied(false), 1600)
    } catch {
      // Clipboard API unavailable (insecure context / permission denied) —
      // the install command is still visible and selectable in the field.
    }
  }

  useGSAP(
    () => {
      const seq = seqRef.current
      const mm = gsap.matchMedia()

      /* Reduced motion gets the composition already assembled and the shot on
         a single representative frame — the whole text block is in the DOM
         unhidden, so this branch has nothing to reveal. See SHOT_STILL for why
         that frame and not either end of the loop. */
      mm.add('(prefers-reduced-motion: reduce)', () => {
        seq?.show(SHOT_STILL)
      })

      mm.add('(prefers-reduced-motion: no-preference)', () => {
        /* Beats 2 and 3 are hidden here rather than in CSS: if the bundle
           never arrives, the hero still renders its full copy.

           No blur on these two. The one-shot entrance below can afford one —
           it runs once, for a second, before the pin exists. These are driven
           by the scrub, so a blur here is a filter the compositor re-resolves
           over a 500px-wide text block on every frame that the canvas is also
           re-compositing two full-viewport blits on. It is the most expensive
           thing in the timeline and it is spent on the least legible part of
           it; transform and opacity carry the same accretion for nothing. */
        gsap.set('[data-beat]', { opacity: 0, y: 20 })
        gsap.set('[data-rail-fill]', { scaleY: 0, transformOrigin: 'top center' })
        // The pin is the known duration of the animation, so the hint is
        // correct for exactly as long as it is set — reverted with the context.
        gsap.set('[data-beat], [data-parallax]', { willChange: 'transform, opacity' })

        gsap.from('[data-enter]', {
          y: 22,
          opacity: 0,
          filter: 'blur(8px)',
          duration: 1,
          ease: 'expo.out',
          stagger: 0.08,
          clearProps: 'filter',
        })

        /* One timeline owns the whole pin: the shot scrubs, the opening block
           drifts, and the two later beats settle in behind it. The text
           accretes rather than swapping — nothing that has been read is taken
           away, and the pin ends on the complete hero.

           The shot is strictly position-mapped: pin progress is frame
           position, through `shotPosition` and nothing else. Scroll down and
           the frames advance; scroll up and they rewind, because scrolling up
           is the visitor undoing their own scroll and a shot that kept
           advancing there would feel unhooked from the page.

           `shotPosition` is monotonic, so all of that survives the accent —
           it redistributes how much scroll each part of the render costs, and
           never changes which direction it runs. No easing on the tween
           itself: the curve belongs to the map, where it is aimed at a named
           span of footage, rather than to the timeline, where it would
           silently re-time the text beats as well. */
        const shot = { p: 0 }

        function advance() {
          seq?.show(shotPosition(shot.p))
        }

        const tl = gsap.timeline({
          defaults: { ease: 'none' },
          scrollTrigger: {
            trigger: pinRef.current,
            start: 'top top',
            end: PIN_LENGTH,
            pin: true,
            scrub: SCRUB,
            anticipatePin: 1,
          },
        })

        /* fromTo, not to: `advance` reads `shot.p` on every tick, so the
           tween's start value has to be pinned to 0 rather than recorded from
           whatever the object happens to hold when the timeline first
           renders. */
        tl.fromTo(
          shot,
          { p: 0 },
          {
            p: 1,
            duration: 1,
            onUpdate: advance,
          },
          0
        )
          .to('[data-parallax]', { y: -56, duration: 1 }, 0)
          .fromTo('[data-rail-fill]', { scaleY: 0 }, { scaleY: 1, duration: 1 }, 0)
          /* Both beats are in by 0.48, which the accent map makes exactly the
             right place for them: frame 90 lands at 55% of the pin, so the
             copy finishes arriving just before the reach starts to slow, and
             the whole back half of the pin is the finished composition against
             the arrival, rather than text still landing over the one moment
             worth watching.

             `power2.out` over 0.22, not `expo.out` over 0.16. An exponential
             ease is the right curve for something the visitor watches play,
             because it arrives fast and settles; under a scrub the visitor
             *is* the clock, and it inverts — nine tenths of the movement is
             spent in the first tenth of the scroll, so the beat snaps in over
             a few pixels and then crawls. A gentler curve over a longer share
             of the pin is what a scrubbed arrival has to be to read as one. */
          .to(
            '[data-beat="subhead"]',
            { opacity: 1, y: 0, duration: 0.22, ease: 'power2.out' },
            0.06
          )
          .to(
            '[data-beat="install"]',
            { opacity: 1, y: 0, duration: 0.22, ease: 'power2.out' },
            0.26
          )
          /* The rail exists to say the pin runs past one viewport, so it is
             spent once that is no longer news. It clears through 0.74–0.9, a
             fade with room to be one; a tenth-of-a-timeline exit read as a
             blink.

             That window is inside the accent, which is the point. The rail is
             a progress meter, and a progress meter is the one thing that can
             flatten the emphasis — it reports that the scroll is still moving
             at its usual rate while the picture deliberately is not, and
             invites the visitor to read the slowdown as the page lagging. It
             is gone by frame 112, so the shot has the frame to itself through
             the arrival. */
          .to('[data-rail]', { opacity: 0, duration: 0.16 }, 0.74)
      })

      /* The pin's length is in vh, but the text block's height is not — a
         late font swap would move the start position under the pin. */
      document.fonts?.ready.then(() => ScrollTrigger.refresh())

      return () => mm.revert()
    },
    { scope: rootRef }
  )

  return (
    <section id="hero" ref={rootRef} aria-label="Aegis">
      {/* The pin's own ground stays the page's black. The shot covers it whole
          on desktop, so what this colour actually paints is the strip below
          the phone's band — and that strip is continuous with the section
          underneath, not with the shot, which is also why both bottom fades
          terminate on #000 rather than on the substrate. */}
      <div
        ref={pinRef}
        className="relative flex h-svh w-full flex-col overflow-hidden bg-black md:block"
      >
        {/* Phone: a full-bleed band under the nav. Desktop: the whole
            viewport, with the type reading against the shot's dark left.

            The band was a fixed `54svh`, which fixed the wrong half. The copy
            below it is a stack of known blocks — eyebrow, headline, actions,
            subhead, install field — and on a 667px phone that stack is taller
            than the 46svh the band left it, so the install field rendered at
            full opacity *below* the pinned viewport and was clipped by the
            pin's `overflow-hidden`. Nothing could scroll it into view either,
            because the pin holds the hero still for its whole length.

            So the copy is now what is measured and the band takes the
            remainder: `flex-1` against a `shrink-0` sibling. `min-h-[32svh]`
            keeps the shot from collapsing to a strip on a very short viewport
            — past that point the phone is short enough that the reach reads
            better cropped than the sentence does truncated. Desktop is
            untouched: the band goes back to `absolute inset-0` at `md`. */}
        <div className="relative min-h-[30svh] flex-1 md:absolute md:inset-0 md:h-full md:flex-none">
          <FrameSequence
            ref={seqRef}
            label={SHOT_LABEL}
            className="absolute inset-0 h-full w-full"
            range={SHOT_RANGE}
            fit="cover"
            focusX={isDesktop ? FOCUS_DESKTOP : FOCUS_MOBILE}
            focusY={0.5}
            zoom={SHOT_ZOOM}
            shiftY={isDesktop ? SHIFT_DESKTOP : SHIFT_MOBILE}
            backdrop={SUBSTRATE}
          />
          {/* Dropping the shot exposes a band of substrate exactly `shiftY`
              tall. Both masks are sized from the same constants rather than a
              fixed height — opaque across the band, fading only past it — so
              they cannot drift apart when a shift is retuned. On the phone the
              join sits under the nav as well, but the band is taller there and
              the mask still does the work. */}
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-x-0 top-0 hidden md:block"
            style={{
              height: `${(SHIFT_DESKTOP + SEAM_FADE) * 100}%`,
              background: `linear-gradient(to bottom, ${SUBSTRATE} 0%, ${SUBSTRATE} ${
                (SHIFT_DESKTOP / (SHIFT_DESKTOP + SEAM_FADE)) * 100
              }%, transparent 100%)`,
            }}
          />
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-x-0 top-0 md:hidden"
            style={{
              height: `${(SHIFT_MOBILE + SEAM_FADE) * 100}%`,
              background: `linear-gradient(to bottom, ${SUBSTRATE} 0%, ${SUBSTRATE} ${
                (SHIFT_MOBILE / (SHIFT_MOBILE + SEAM_FADE)) * 100
              }%, transparent 100%)`,
            }}
          />
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-x-0 bottom-0 h-48 md:hidden"
            style={{
              background:
                'linear-gradient(to top, #000 0%, #000 34%, rgba(0,0,0,0.72) 62%, transparent 100%)',
            }}
          />
        </div>

        {/* Desktop scrim. The pane's glow is the brightest thing on the page,
            so the type gets a graded substrate behind it rather than a flat
            panel — the shot stays whole and the copy clears its ratio
            everywhere.

            This render throws a wide, soft column of bloom down the middle of
            the field rather than keeping it around the pane, so the grade has
            to manufacture the ratio and not merely guarantee it. The headline
            runs to a 660px measure, which at a 1200px gutter reaches into that
            bloom; at a lighter grade its first line measures 2.7:1. It holds
            most of its weight out to 68% and is clear by 88% — the pane itself
            sits past 90% and is never touched. */}
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 hidden md:block"
          style={{
            /* Every stop is the substrate at a different weight, so they are
               generated from `SUBSTRATE` rather than written out as six
               `rgba(4,9,14,…)` literals that a retune of the ground colour
               would silently leave behind. */
            background: `linear-gradient(95deg, ${SCRIM_STOPS})`,
          }}
        />

        {/* Desktop dissolve. Even shifted down, the shot runs past the bottom
            of the viewport with the lit cable fan still in frame — the pin's
            bottom edge cut straight through it. Grading the last fifth out
            lands the shot on the page's own black instead of guillotining it,
            and the fan falling into the dark is what the shot wants anyway.

            It has to come after the scrim, not inside the shot's own layer:
            the scrim is 94% opaque substrate down the left edge, so anything
            it paints over ends on #04090e rather than #000 and the join with
            the section below shows as a step across the full width. Last in
            source order, this grades the scrim out too. */}
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-x-0 bottom-0 hidden h-[22vh] md:block"
          style={{
            background:
              'linear-gradient(to top, #000 0%, rgba(0,0,0,0.88) 30%, rgba(0,0,0,0.5) 62%, transparent 100%)',
          }}
        />

        <div className="relative flex shrink-0 items-start pt-4 pb-6 md:absolute md:inset-0 md:h-full md:items-center md:py-0">
          <div className="relative mx-auto w-full max-w-[1200px] px-gutter">
            {/* Scroll rail — a hairline that measures the pin, hung on the
                content gutter so it belongs to the text column rather than
                floating at the window edge. It is the only affordance telling
                the visitor the pin runs past one viewport.

                Its fill is Cyan Neon: the rail reports live state, which is
                the one job that colour has here. The action next to it is
                Electric Blue, so the two accents never mean the same thing. */}
            <div
              data-rail
              aria-hidden="true"
              className="pointer-events-none absolute top-1/2 left-0 hidden w-px -translate-y-1/2 bg-night-edge md:block"
              style={{ height: '120px' }}
            >
              <div data-rail-fill className="h-full w-full bg-cyan-neon" />
            </div>

            <div data-parallax className="max-w-[660px]">
              {/* Keep the product name separate from the value proposition so
                  the headline can state the category, audience, and purpose
                  without repeating the brand. */}
              {/* The brand signature: the one surface that carries the
                  handwriting accent face. It is a label, not a terminal
                  caption, so it drops the mono + uppercase treatment and
                  reads as a signed name above the headline. */}
              <p
                data-enter
                className="mb-4 font-accent text-[24px] leading-none text-cloud-mute md:mb-7 md:text-[28px]"
              >
                Aegis ShellGuard
              </p>

              <h1
                data-enter
                className="font-inter-variable text-[40px] font-bold leading-[1.02] tracking-[-0.9px] text-cloud-mute sm:text-heading sm:tracking-heading md:text-heading-lg md:tracking-heading-lg lg:text-display lg:tracking-display"
              >
                A shell guardrail for{' '}
                <span className="text-cloud">
                  AI coding agents.
                </span>
              </h1>

              <div data-enter className="mt-5 flex flex-wrap md:mt-9 items-center gap-3">
                <a
                  href="https://github.com/IliasAlmerekov/aegis-shellguard"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="cta-primary inline-flex h-10 items-center rounded-md bg-electric px-4 font-inter-variable text-sm text-night-void focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-neon focus-visible:ring-offset-2 focus-visible:ring-offset-night-void"
                >
                  Install Aegis — free
                </a>
                <a
                  href="#how-it-works"
                  className="cta-ghost inline-flex h-10 items-center rounded-md border border-night-edge px-4 font-inter-variable text-sm text-cloud-dim focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-neon focus-visible:ring-offset-2 focus-visible:ring-offset-night-void"
                >
                  See how it works
                </a>
              </div>

              <p
                data-beat="subhead"
                className="mt-4 max-w-[500px] font-inter-variable md:mt-8 text-body-sm leading-body-sm tracking-body-sm text-cloud-dim md:text-body-lg md:leading-body-lg md:tracking-body-lg"
              >
                Aegis checks every command before it runs. Safe commands pass;
                risky ones wait for you; catastrophic ones are blocked.
              </p>

              {/* Night Black proper as the fill, at the weight that lets the
                  shot read through it — the field is a surface on the footage,
                  not a panel laid over it. */}
              <div
                data-beat="install"
                className="mt-4 flex w-full max-w-[420px] items-center md:mt-8 gap-3 rounded-md border border-night-edge/80 bg-night/60 px-3.5 py-2.5"
              >
                <span
                  aria-hidden="true"
                  className="font-berkeley-mono text-xs leading-none text-cyan-neon"
                >
                  $
                </span>
                <code className="min-w-0 flex-1 truncate font-berkeley-mono text-xs leading-none text-cloud">
                  {INSTALL_CMD}
                </code>
                <button
                  className="copy-btn -mr-1 shrink-0 rounded-sm p-1 text-cloud-faint transition-colors duration-150 hover:text-cloud focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-cyan-neon cursor-pointer"
                  aria-label="Copy install command"
                  onClick={handleCopy}
                >
                  {copied ? (
                    <svg
                      key="check"
                      className="copy-icon-pop stroke-cyan-neon"
                      width="13"
                      height="13"
                      viewBox="0 0 24 24"
                      fill="none"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      aria-hidden="true"
                    >
                      <path d="M20 6L9 17l-5-5" />
                    </svg>
                  ) : (
                    <svg
                      key="copy"
                      width="13"
                      height="13"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      aria-hidden="true"
                    >
                      <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                    </svg>
                  )}
                </button>
                <span className="sr-only" role="status">
                  {copied ? 'Copied to clipboard' : ''}
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
