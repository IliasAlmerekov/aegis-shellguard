'use client'

import dynamic from 'next/dynamic'
import { useCallback, useRef, useState } from 'react'

import { gsap, ScrollTrigger, useGSAP } from '../../lib/gsap'
import { SCROLL } from '../../lib/scene/config'
import { clamp01, densityMap, smoothstep, spanProgress } from '../../lib/scene/progress'

const INSTALL_CMD = 'npm i -g @iliasalmerekov/aegis'

/**
 * The scene is loaded dynamically and on the client only: a static export has
 * nothing to render WebGL with, while the text has to be in the markup from
 * the first paint.
 *
 * There is deliberately no placeholder image behind the canvas. A baked poster
 * would drift away from the material at the first edit, and the discrepancy
 * would be discovered a month later on somebody's phone.
 */
const HeroScene = dynamic(
  () => import('../hero/HeroScene').then((m) => m.HeroScene),
  { ssr: false }
)

ScrollTrigger.config({ ignoreMobileResize: true })

export function Hero() {
  const [copied, setCopied] = useState(false)
  const [sceneReady, setSceneReady] = useState(false)
  const rootRef = useRef<HTMLElement>(null)
  const pinRef = useRef<HTMLDivElement>(null)

  const handleSceneReady = useCallback(() => setSceneReady(true), [])

  /**
   * Pin progress, 0..1. The only channel between the scroll and the scene.
   *
   * A ref, not state: the scrub arrives every frame, and `setState` every
   * frame would re-render the whole subtree for the sake of one number that
   * only `useFrame` reads.
   */
  const progress = useRef(0)

  useGSAP(
    () => {
      const mm = gsap.matchMedia()

      /**
       * Under `prefers-reduced-motion` the pin is not created at all, rather
       * than created with zero durations: a pin is itself page motion, and a
       * visitor who has turned that off has no use for it in any form. The
       * scene meanwhile stays at full quality, simply motionless.
       */
      mm.add('(prefers-reduced-motion: no-preference)', () => {
        const map = densityMap(SCROLL.accent)

        /**
         * The markup and the scene run on **one clock**.
         *
         * The timeline is deliberately not scrubbed by ScrollTrigger: a scrub
         * would follow pin progress, whereas the scene reads progress passed
         * through the density map. Over the accent the map takes the extra
         * scroll, so the darkness used to start creeping in before the scene
         * had reached its exit. Here the timeline's position is set by the
         * same mapped number that goes into the scene, and the two cannot
         * drift apart.
         */
        const timeline = gsap.timeline({ paused: true, defaults: { ease: 'none' } })

        /**
         * The curtain is the only thing driven by **raw** scroll.
         *
         * Everything else here runs on smoothed progress, and rightly so: a
         * mouse wheel gives a fifty-pixel jump with no intermediate values,
         * and the scene needs something to fill the frames in between. But
         * smoothing is lag, and the pin releases on exactly its last pixel and
         * knows no lag at all. A curtain running with everyone else physically
         * cannot finish closing by the handoff: measurement put it at one at
         * 103.3% of the pin, when the page was already moving.
         *
         * The curtain has the right to leave the common clock, and it is the
         * same right the scrim has: it is not an object in space but a
         * darkening of the frame. Objects must not drift apart from one
         * another; a darkening has nothing to drift from.
         *
         * `quickSetter` instead of a tween: the value is rewritten on every
         * frame of the scroll, and setting up an animation for it would mean
         * building and discarding an object each time.
         */
        const setCurtain = gsap.quickSetter('[data-hero-curtain]', 'opacity')

        ScrollTrigger.create({
          trigger: pinRef.current,
          start: 'top top',
          end: SCROLL.pin,
          pin: true,
          scrub: SCROLL.scrub,
          anticipatePin: 1,
          onUpdate: (self) => {
            const scene = map(self.progress)
            progress.current = self.progress
            timeline.progress(scene)

            // The raw pin position: `self.progress` is already smoothed, while
            // the bounds are document pixels and `self.scroll()` returns the
            // real position.
            const raw = (self.scroll() - self.start) / (self.end - self.start)
            setCurtain(spanProgress(map(clamp01(raw)), SCROLL.exit))
          },
          /**
           * The fly-past depth is computed from the frame height, and the
           * frame height changes.
           *
           * Three steps, and their order is mandatory. `invalidate` makes GSAP
           * re-read the function values below, but it also makes it forget
           * where the animation started from — and it recalls that from the
           * DOM state at the first render. If the recalculation caught the
           * copy mid-flight (rotating a phone halfway through the pin is an
           * everyday event), the middle of the flight would be recorded as its
           * start, and the copy could never return to rest. Hence the return
           * to zero first, and only then the re-read.
           *
           * The third step restores the position: the timeline is paused, we
           * set its position ourselves, and without this it would be left
           * standing at zero.
           */
          onRefresh: (self) => {
            timeline.progress(0)
            timeline.invalidate()
            timeline.progress(map(self.progress))
            setCurtain(spanProgress(map(self.progress), SCROLL.exit))
          },
        })

        const flySpan = SCROLL.approach.to - SCROLL.approach.from

        /**
         * The copy flies past the camera.
         *
         * It is driven by **the same phase and the same curve** as the camera
         * approach: the ease is exactly the `smoothstep` that `stageAt` uses
         * for `approach`, and the span is the same `SCROLL.approach`. So the
         * text and the stone cannot drift apart: they do not have two
         * coordinated animations, they have one.
         *
         * Two quantities move the copy, and both are taken from the camera's
         * motion: depth is the approach, `y` is the aim rising from zero to
         * the cube's centre. There is no scaling by hand here: the
         * magnification is done entirely by perspective, because the motion of
         * a fronto-parallel plane in depth projects to exactly a uniform
         * magnification about the vanishing point.
         *
         * Depth alone is not enough, and that has been verified frame by
         * frame: a copy block half a frame tall straddles the vanishing point,
         * so its upper half diverges **upward** and the block never leaves
         * through the bottom edge at any magnification. The rise carries it
         * out whole.
         *
         * The quantities are functions rather than numbers because they are
         * computed from the frame height, and the height changes. `onRefresh`
         * above re-reads them.
         *
         * There are still no filters: a blur would be recomputed on every
         * frame, on a canvas that is already busy with the scene.
         */
        timeline.to(
          '[data-hero-copy]',
          {
            z: () => SCROLL.fly.travel * SCROLL.fly.perspective * window.innerHeight,
            y: () => SCROLL.fly.rise * window.innerHeight,
            ease: smoothstep,
            duration: flySpan,
          },
          SCROLL.approach.from
        )

        /**
         * The copy stops catching the pointer the moment it starts moving.
         *
         * The departed block is clipped by the pin's edge, but it is clipped
         * *visually*: the link and the copy button would remain under the
         * cursor in whatever band of the frame they were carried to. This used
         * to be hidden behind zero opacity, which does not cancel clicks
         * either.
         */
        timeline.set('[data-hero-copy]', { pointerEvents: 'none' }, SCROLL.approach.from)

        // Insurance for viewports where the divergence did not carry the block
        // out in full.
        timeline.to(
          '[data-hero-copy]',
          {
            opacity: 0,
            duration: (SCROLL.fly.fade.to - SCROLL.fly.fade.from) * flySpan,
          },
          SCROLL.approach.from + SCROLL.fly.fade.from * flySpan
        )

        // The scrim is a darkening of the frame, not an object in it: it fades,
        // it does not fly.
        timeline.to(
          '[data-hero-floor]',
          { opacity: 0, duration: SCROLL.floor.to - SCROLL.floor.from },
          SCROLL.floor.from
        )

        // The curtain is deliberately absent from the timeline: it is driven
        // by raw scroll in `onUpdate` above — see the note on `setCurtain`.

        // The timeline must be exactly 1 long: its position is set as a
        // fraction, not in seconds. The filler makes up the length when the
        // spans do not cover it.
        timeline.set({}, {}, 1)

        // The curtain is written as an inline style, bypassing the timeline,
        // so it has to be cleared by hand as well: `revert` only knows about
        // tweens.
        return () => {
          gsap.set('[data-hero-curtain]', { clearProps: 'opacity' })
        }
      })

      /**
       * Recomputing after the fonts load is mandatory.
       *
       * ScrollTrigger measures the page at the moment it is created. The
       * variable faces arrive later and change the height of the text block,
       * and with it the start and end of the pin. Without a refresh the
       * measurements are left over from the old layout and the pin releases in
       * the wrong place: the page looks shifted by the height of the
       * discrepancy.
       */
      document.fonts?.ready.then(() => ScrollTrigger.refresh())

      return () => {
        mm.revert()
        progress.current = 0
      }
    },
    { scope: rootRef }
  )

  const handleCopy = async () => {
    try {
      await navigator.clipboard?.writeText(INSTALL_CMD)
      setCopied(true)
      setTimeout(() => setCopied(false), 1600)
    } catch {
      // The clipboard may be unavailable; the string is visible and can be
      // selected by hand.
    }
  }

  return (
    <section id="hero" ref={rootRef} aria-label="Aegis" className="relative bg-black">
      <div
        ref={pinRef}
        className="relative flex h-svh w-full flex-col overflow-hidden"
      >
        {/* The field the stone hangs in. In the reference the background is
            not black but deep blue, and that works: a black background makes
            the object look cut out, a lit one makes it hang in space. The
            gradient is in the markup, not in the scene: it must not cost a
            frame.

            Darkened, and darkened from the outside in rather than uniformly.
            The former bounds — 120% 80% with the last stop at 100% — meant the
            ramp never finished inside the frame: light was left in the
            corners, and the field read not as space around the stone but as a
            blue backing under it. The pocket behind the stone is needed — it
            is what keeps the stone hanging — so the first stop is lowered most
            gently and the outer ones more: what darkens is what is around, not
            what is behind.

            Full black is now reached at 96%, which means it is present in the
            frame rather than beyond its edge. That is also what makes the join
            with the section below seamless: it stands on pure #000. */}
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-0"
          style={{
            background:
              'radial-gradient(98% 64% at 50% 28%, #0a1630 0%, #050a1a 32%, #01030a 56%, #000000 82%)',
          }}
        />
        {/* The wrapper positions it, not the Canvas itself: R3F puts an inline
            `position: relative` on its root div, and that would beat the
            `absolute` class — the canvas would become an ordinary flex-column
            item and take only whatever height was left after the text. */}
        <div className="absolute inset-0">
          <HeroScene progress={progress} onReady={handleSceneReady} />
        </div>

        {/* The scene's maps and shaders are needed above the fold, so they are
            preloaded from the layout. While the GPU assembles the first frame
            the visitor sees a loading state rather than an empty space; the
            layer only disappears after the first paint of the finished stone —
            see FirstFrame in HeroScene. */}
        <div
          aria-hidden={sceneReady}
          className={`pointer-events-none absolute inset-0 z-[5] flex items-start justify-center pt-[27svh] bg-[#00050a] transition-opacity duration-300 ${
            sceneReady ? 'opacity-0' : 'opacity-100'
          }`}
        >
          <div
            role="status"
            aria-live="polite"
            className="flex items-center gap-2 font-berkeley-mono text-[11px] tracking-[0.12em] text-cloud-faint"
          >
            <span aria-hidden="true" className="h-1.5 w-1.5 animate-pulse rounded-full bg-cyan-neon" />
            Initializing protection surface
          </div>
        </div>

        {/* A vignette in place of the former asymmetric scrim: the composition
            is centred now, and the headline's contrast is guaranteed by
            darkening toward the bottom of the frame rather than by a gradient
            off to one side. */}
        <div
          data-hero-floor
          aria-hidden="true"
          className="pointer-events-none absolute inset-x-0 bottom-0 h-[52%]"
          style={{
            background:
              'linear-gradient(to top, #000 0%, rgba(0,0,0,0.92) 26%, rgba(0,0,0,0.62) 56%, transparent 100%)',
          }}
        />

        <div
          data-hero-curtain
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 z-20 opacity-0"
          style={{ background: SCROLL.fadeTo }}
        />

        {/*
          The wrapper exists for the vanishing point, and for nothing else.

          `perspective-origin` is computed as a percentage **of the element
          carrying the perspective**, not of the frame. A wrapper sized to the
          copy would put the vanishing point inside the text, and the copy
          would diverge about its own middle rather than about the centre of
          the frame. So the wrapper is stretched across the whole pin: only
          then does the height fraction in `SCROLL.fly.originY` mean a fraction
          of the frame. The value happens to match the CSS default — but here
          the coincidence is a consequence, not a reason: the centre of the
          frame is the principal point of the projection.

          The distance to the projection plane is given in `svh` rather than in
          pixels: that way it survives a phone rotating and an address bar
          appearing without a single recalculation in JS. Only the fly-past
          depth is computed in pixels, and GSAP re-reads it on `onRefresh`.

          The wrapper does not catch the pointer: it covers the whole frame,
          the stone included, and the stone has to answer the cursor.
        */}
        <div
          className="pointer-events-none absolute inset-0 z-10 flex flex-col justify-end"
          style={{
            perspective: `${SCROLL.fly.perspective * 100}svh`,
            perspectiveOrigin: `50% ${SCROLL.fly.originY * 100}%`,
          }}
        >
        <div
          data-hero-copy
          className="pointer-events-auto flex w-full justify-center px-gutter pb-[8svh]"
        >
          <div className="flex max-w-[720px] flex-col items-center text-center">
            <h1
              data-enter
              className="font-inter-variable text-[40px] font-bold leading-[1.02] tracking-[-0.9px] text-cloud-mute sm:text-heading sm:tracking-heading md:text-heading-lg md:tracking-heading-lg lg:text-display lg:tracking-display"
            >
              A shell guardrail for <span className="text-cloud">AI coding agents.</span>
            </h1>

            <p
              data-enter
              className="mt-5 max-w-[520px] font-inter-variable text-body-sm leading-body-sm tracking-body-sm text-cloud-dim md:mt-6 md:text-body-lg md:leading-body-lg md:tracking-body-lg"
            >
              Aegis checks every command before it runs. Safe commands pass; risky
              ones wait for you; catastrophic ones are blocked.
            </p>

            <a
              data-enter
              href="https://github.com/IliasAlmerekov/aegis-shellguard"
              target="_blank"
              rel="noopener noreferrer"
              className="cta-primary mt-7 inline-flex h-10 items-center rounded-md bg-electric px-4 font-inter-variable text-sm text-cloud focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-neon focus-visible:ring-offset-2 focus-visible:ring-offset-night-void md:mt-8"
            >
              Install Aegis — free
            </a>

            <div
              data-enter
              className="mt-4 flex w-full max-w-[420px] items-center gap-3 rounded-md border border-night-edge/80 bg-night/60 px-3.5 py-2.5"
            >
              <span
                aria-hidden="true"
                className="font-berkeley-mono text-xs leading-none text-cyan-neon"
              >
                $
              </span>
              <code className="min-w-0 flex-1 truncate text-left font-berkeley-mono text-xs leading-none text-cloud">
                {INSTALL_CMD}
              </code>
              <button
                className="copy-btn -mr-1 shrink-0 cursor-pointer rounded-sm p-1 text-cloud-faint transition-colors duration-150 hover:text-cloud focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-cyan-neon"
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
