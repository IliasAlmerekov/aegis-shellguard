'use client'

import dynamic from 'next/dynamic'
import { useCallback, useRef, useState } from 'react'

import { gsap, ScrollTrigger, useGSAP } from '../../lib/gsap'
import { SCROLL } from '../../lib/scene/config'

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
        // The scroll moves the stone and nothing else: the copy stands, the
        // frame keeps its light, and the pin lets go as the stone closes.
        ScrollTrigger.create({
          trigger: pinRef.current,
          start: 'top top',
          end: SCROLL.pin,
          pin: true,
          scrub: SCROLL.scrub,
          anticipatePin: 1,
          onUpdate: (self) => {
            progress.current = self.progress
          },
        })
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
          className="pointer-events-none absolute inset-0 z-10 flex flex-col justify-start sm:justify-end"
          style={{
            perspective: `${SCROLL.fly.perspective * 100}svh`,
            perspectiveOrigin: `50% ${SCROLL.fly.originY * 100}%`,
          }}
        >
        <div
          data-hero-copy
          // On a phone the stone is scaled to the frame's width and lifted
          // (see `fit.ts`), so the copy hangs from the top by the same two
          // measures, a little under the stone. The second term keeps the
          // block on a short screen: 24rem is its own height.
          className="pointer-events-auto flex w-full justify-center px-gutter pt-[min(31svh_+_38vw,100svh_-_24rem)] sm:pt-0 sm:pb-[8svh]"
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
