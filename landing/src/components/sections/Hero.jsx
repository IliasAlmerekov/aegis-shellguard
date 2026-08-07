import { useEffect, useRef, useState } from 'react'
import gsap from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'
import { useGSAP } from '@gsap/react'
import { FrameSequence } from '../ui/FrameSequence'

gsap.registerPlugin(useGSAP, ScrollTrigger)

const INSTALL_CMD = 'npm i -g @iliasalmerekov/aegis'

const SHOT_LABEL =
  'A machine hand rises out of the dark toward a suspended, fractured pane of light and stops just short of touching it.'

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

/* The shot is framed with the plate near the top of the 16:9 field, which left
   it pinned against the nav. Dropping the whole window gives it a little air;
   what leaves the bottom of the frame is the tail of the cabling, already
   fading to black.

   How far it can drop is capped by the source, not by taste: the plate's top
   edge sits about 4% from the top of the field, so there is almost no image
   above it, and the exposed band is black because there is nothing else to
   put there. Past a small nudge it stops reading as air and starts reading as
   a bar between the nav and the shot. Zooming instead of shifting does not
   buy the drop either — anchored to the top edge it takes roughly 3.4x to
   move the plate the same distance, which throws the hand and most of the arm
   out of frame. These values are the most this composition takes. */
const SHOT_ZOOM = 1
const SHIFT_DESKTOP = 0.1
const SHIFT_MOBILE = 0.12

/* How far past the exposed band the mask keeps fading, as a share of frame
   height. Enough to clear the shot's top row without eating into the plate. */
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

  const handleCopy = () => {
    navigator.clipboard?.writeText(INSTALL_CMD)
    setCopied(true)
    setTimeout(() => setCopied(false), 1600)
  }

  useGSAP(
    () => {
      const seq = seqRef.current
      const mm = gsap.matchMedia()

      /* Reduced motion gets the composition already assembled and the shot
         at its last frame — the whole text block is in the DOM unhidden, so
         this branch has nothing to reveal. */
      mm.add('(prefers-reduced-motion: reduce)', () => {
        seq?.show(1)
      })

      mm.add('(prefers-reduced-motion: no-preference)', () => {
        /* Beats 2 and 3 are hidden here rather than in CSS: if the bundle
           never arrives, the hero still renders its full copy. */
        gsap.set('[data-beat]', { opacity: 0, y: 24, filter: 'blur(10px)' })
        gsap.set('[data-rail-fill]', { scaleY: 0, transformOrigin: 'top center' })

        gsap.from('[data-enter]', {
          y: 22,
          opacity: 0,
          filter: 'blur(8px)',
          duration: 1,
          ease: 'expo.out',
          stagger: 0.08,
          clearProps: 'filter',
        })

        /* One timeline owns the whole pin: the shot scrubs, the opening
           block drifts, and the two later beats settle in behind it. The
           text accretes rather than swapping — nothing that has been read
           is taken away, and the pin ends on the complete hero. */
        const shot = { p: 0 }
        const tl = gsap.timeline({
          defaults: { ease: 'none' },
          scrollTrigger: {
            trigger: pinRef.current,
            start: 'top top',
            end: '+=150%',
            pin: true,
            scrub: 0.35,
          },
        })

        tl.to(
          shot,
          {
            p: 1,
            duration: 0.88,
            onUpdate: () => seq?.show(shot.p),
          },
          0
        )
          .to('[data-parallax]', { y: -56, duration: 1 }, 0)
          .fromTo('[data-rail-fill]', { scaleY: 0 }, { scaleY: 1, duration: 1 }, 0)
          .to(
            '[data-beat="subhead"]',
            { opacity: 1, y: 0, filter: 'blur(0px)', duration: 0.22, ease: 'expo.out' },
            0.12
          )
          .to(
            '[data-beat="install"]',
            { opacity: 1, y: 0, filter: 'blur(0px)', duration: 0.22, ease: 'expo.out' },
            0.46
          )
          .to('[data-rail]', { opacity: 0, duration: 0.12 }, 0.84)
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
      <div
        ref={pinRef}
        className="relative flex h-svh w-full flex-col overflow-hidden bg-black md:block"
      >
        {/* Phone: a full-bleed band under the nav. Desktop: the whole
            viewport, with the type reading against the shot's dark left. */}
        <div className="relative h-[54svh] shrink-0 md:absolute md:inset-0 md:h-full">
          <FrameSequence
            ref={seqRef}
            label={SHOT_LABEL}
            className="absolute inset-0 h-full w-full"
            fit="cover"
            focusX={isDesktop ? FOCUS_DESKTOP : FOCUS_MOBILE}
            focusY={0.5}
            zoom={SHOT_ZOOM}
            shiftY={isDesktop ? SHIFT_DESKTOP : SHIFT_MOBILE}
          />
          {/* Dropping the shot exposes a black band exactly `shiftY` tall, and
              the frame's own top row is not quite pure black, so the join
              reads as a hard horizontal line. This mask is sized from the same
              constant rather than a fixed height: it stays opaque across the
              band and only then fades, so the two can never drift apart when
              the shift is retuned. */}
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-x-0 top-0 hidden md:block"
            style={{
              height: `${(SHIFT_DESKTOP + SEAM_FADE) * 100}%`,
              background: `linear-gradient(to bottom, #000 0%, #000 ${
                (SHIFT_DESKTOP / (SHIFT_DESKTOP + SEAM_FADE)) * 100
              }%, transparent 100%)`,
            }}
          />
          {/* The band dissolves downward instead of ending on a hard edge. */}
          {/* Same join at the top of the phone's band, hidden under the nav
              rather than faded, plus the band's own dissolve at the bottom. */}
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-x-0 top-0 md:hidden"
            style={{
              height: `${(SHIFT_MOBILE + SEAM_FADE) * 100}%`,
              background: `linear-gradient(to bottom, #000 0%, #000 ${
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

        {/* Desktop scrim. The plate's glow is the brightest thing on the page,
            so the type gets a graded black behind it rather than a flat panel
            — the shot stays whole and the copy clears its ratio everywhere.
            The grade holds out to 68% of the width because the headline runs
            to a 660px measure, which reaches into the plate's bloom; at a
            lighter grade the first line measures 2.7:1. The plate itself sits
            past 90%, so it is untouched. */}
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 hidden md:block"
          style={{
            background:
              'linear-gradient(95deg, #000 0%, rgba(0,0,0,0.96) 34%, rgba(0,0,0,0.82) 52%, rgba(0,0,0,0.34) 68%, rgba(0,0,0,0.08) 80%, transparent 88%)',
          }}
        />

        <div className="relative flex min-h-0 flex-1 items-start pt-2 md:absolute md:inset-0 md:h-full md:items-center md:pt-0">
          <div className="relative mx-auto w-full max-w-[1200px] px-6">
            {/* Scroll rail — a hairline that measures the shot, hung on the
                content gutter so it belongs to the text column rather than
                floating at the window edge. It is the only affordance telling
                the visitor the pin runs 150vh. */}
            <div
              data-rail
              aria-hidden="true"
              className="pointer-events-none absolute top-1/2 left-0 hidden w-px -translate-y-1/2 bg-iron md:block"
              style={{ height: '120px' }}
            >
              <div data-rail-fill className="h-full w-full bg-steel" />
            </div>

            <div data-parallax className="max-w-[660px]">
              {/* Keep the product name separate from the value proposition so
                  the headline can state the category, audience, and purpose
                  without repeating the brand. */}
              <p
                data-enter
                className="mb-5 font-berkeley-mono text-[11px] leading-none tracking-[0.08em] text-steel uppercase md:mb-6 md:text-xs"
              >
                Aegis ShellGuard
              </p>

              <h1
                data-enter
                className="font-inter-variable text-[40px] leading-[1.02] tracking-[-0.9px] text-steel sm:text-heading sm:tracking-heading md:text-heading-lg md:tracking-heading-lg lg:text-display lg:tracking-display"
                style={{ fontWeight: 400 }}
              >
                A shell guardrail for{' '}
                <span className="text-haze" style={{ fontWeight: 590 }}>
                  AI coding agents.
                </span>
              </h1>

              <div data-enter className="mt-7 flex flex-wrap md:mt-9 items-center gap-3">
                <a
                  href="https://github.com/IliasAlmerekov/aegis-shellguard"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="cta-primary inline-flex h-10 items-center rounded-md bg-steel px-4 font-inter-variable text-sm text-pitch focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-haze focus-visible:ring-offset-2 focus-visible:ring-offset-pitch"
                  style={{ fontWeight: 510, letterSpacing: '-0.011em' }}
                >
                  Install Aegis — free
                </a>
                <a
                  href="#how-it-works"
                  className="cta-ghost inline-flex h-10 items-center rounded-md border border-gunmetal px-4 font-inter-variable text-sm text-haze focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-haze focus-visible:ring-offset-2 focus-visible:ring-offset-pitch"
                  style={{ fontWeight: 400, letterSpacing: '-0.011em' }}
                >
                  See how it works
                </a>
              </div>

              <p
                data-beat="subhead"
                className="mt-6 max-w-[500px] font-inter-variable md:mt-8 text-body-sm leading-body-sm tracking-body-sm text-steel md:text-body-lg md:leading-body-lg md:tracking-body-lg"
              >
                Aegis checks every command before it runs. Safe commands pass;
                risky ones wait for you; catastrophic ones are blocked.
              </p>

              <div
                data-beat="install"
                className="mt-6 flex w-full max-w-[420px] items-center md:mt-8 gap-3 rounded-md border border-gunmetal/70 px-3.5 py-2.5"
                style={{ backgroundColor: 'rgba(31,44,53,0.55)' }}
              >
                <span
                  aria-hidden="true"
                  className="font-berkeley-mono text-xs leading-none text-steel"
                >
                  $
                </span>
                <code className="min-w-0 flex-1 truncate font-berkeley-mono text-xs leading-none text-haze">
                  {INSTALL_CMD}
                </code>
                <button
                  className="copy-btn -mr-1 shrink-0 rounded-sm p-1 text-steel transition-colors duration-150 hover:text-haze focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-haze cursor-pointer"
                  aria-label="Copy install command"
                  onClick={handleCopy}
                >
                  {copied ? (
                    <svg
                      key="check"
                      className="copy-icon-pop"
                      width="13"
                      height="13"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="#b3c2cb"
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
