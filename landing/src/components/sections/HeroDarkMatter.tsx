'use client'

import { useEffect, useRef, useState } from 'react'
import { ScrollTrigger, useGSAP } from '@/lib/gsap'

import { scroll } from '@/lib/scene/config'
import { cameraState } from '@/lib/scene/progress'
import { HeroScene } from '../hero/HeroScene'

const INSTALL_CMD = 'npm i -g @iliasalmerekov/aegis'

export function HeroDarkMatter() {
  const rootRef = useRef<HTMLElement>(null)
  const pinRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLDivElement>(null)

  /* The scroll position the scene reads. A ref, not state: ScrollTrigger
     updates this on every scroll frame, and re-rendering React to move a
     camera would cost more than the camera move. */
  const progressRef = useRef(0)

  const [copied, setCopied] = useState(false)
  const [reducedMotion, setReducedMotion] = useState(false)

  useEffect(() => {
    const mql = window.matchMedia('(prefers-reduced-motion: reduce)')
    const onChange = () => setReducedMotion(mql.matches)
    onChange()
    mql.addEventListener('change', onChange)
    return () => mql.removeEventListener('change', onChange)
  }, [])

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
      const trigger = ScrollTrigger.create({
        trigger: rootRef.current,
        start: 'top top',
        /* The pin lasts `screens` viewport heights past the top. The scene's
           own progress is derived from the same number in config, so the
           camera finishes its move exactly as the pin releases. */
        end: `+=${scroll.screens * 100}%`,
        pin: pinRef.current,
        pinSpacing: true,
        scrub: true,
        onUpdate: (self) => {
          progressRef.current = self.progress

          /* The hero dissolves in the DOM rather than in the shader: the page
             behind it is the same black the scene fades to, so fading the
             canvas element reaches the same image — and it keeps working on
             the still tier, where the shader is no longer drawing frames. */
          const el = canvasRef.current
          if (el) el.style.opacity = String(cameraState(self.progress).opacity)
        },
      })

      return () => trigger.kill()
    },
    { scope: rootRef }
  )

  useEffect(() => {
    /* Fonts land after first paint and change the height of the copy, which
       moves every pin start below it. */
    document.fonts?.ready.then(() => ScrollTrigger.refresh())
  }, [])

  return (
    <section id="hero" ref={rootRef} className="relative z-0">
      <div ref={pinRef} className="relative h-dvh w-full overflow-hidden">
        {/* The scene fills the hero and sits under the copy. The matter is
            framed into the right half by the camera's own offset rather than
            by sizing the canvas, so the composition survives a viewport of
            any proportion. */}
        <div ref={canvasRef} className="absolute inset-0">
          <HeroScene
            progressRef={progressRef}
            prefersReducedMotion={reducedMotion}
          />
        </div>

        <div className="pointer-events-none absolute inset-0 flex items-center">
          <div className="mx-auto w-full max-w-[1200px] px-gutter">
            <div className="pointer-events-auto max-w-[560px]">
              {/* The brand signature: the one surface that carries the
                  handwriting accent face. */}
              <p className="mb-4 font-accent text-[24px] leading-none text-cloud-mute md:mb-7 md:text-[28px]">
                Aegis ShellGuard
              </p>

              <h1 className="font-inter-variable text-[40px] font-bold leading-[1.02] tracking-[-0.9px] text-cloud-mute sm:text-heading sm:tracking-heading md:text-heading-lg md:tracking-heading-lg lg:text-display lg:tracking-display">
                A shell guardrail for{' '}
                <span className="text-cloud">AI coding agents.</span>
              </h1>

              <div className="mt-5 flex flex-wrap items-center gap-3 md:mt-9">
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

              <p className="mt-4 max-w-[500px] font-inter-variable text-body-sm leading-body-sm tracking-body-sm text-cloud-dim md:mt-8 md:text-body-lg md:leading-body-lg md:tracking-body-lg">
                Aegis checks every command before it runs. Safe commands pass;
                risky ones wait for you; catastrophic ones are blocked.
              </p>

              <div className="mt-4 flex w-full max-w-[420px] items-center gap-3 rounded-md border border-night-edge/80 bg-night/60 px-3.5 py-2.5 md:mt-8">
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
