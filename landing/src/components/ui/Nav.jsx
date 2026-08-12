import { useEffect, useRef, useState } from 'react'
import { gsap, ScrollTrigger, useGSAP } from '../../lib/gsap'

const NAV_LINKS = [
  { label: 'Why Aegis', href: '#why-aegis' },
  { label: 'How It Works', href: '#how-it-works' },
  { label: 'Audit Trail', href: '#audit-trail' },
]

// Longest of the burger/panel transitions, so a rapid double-click can't
// retrigger the multi-stage blade keyframes mid-flight (they'd snap back
// to their 0% state instead of reversing smoothly like a transition would).
const NAV_TOGGLE_LOCK_MS = 500

// How far the page must scroll before the chrome condenses. Below this the
// nav stays expanded even while scrolling down, so a nudge at the very top
// of the page does not collapse the links the moment the user moves.
const CONDENSE_THRESHOLD = 80

export function Nav() {
  const [open, setOpen] = useState(false)
  const toggleLocked = useRef(false)

  const headerRef = useRef(null)
  const actionsRef = useRef(null)
  const ctaRef = useRef(null)
  const burgerRef = useRef(null)
  const panelRef = useRef(null)
  const toggleOpen = () => {
    if (toggleLocked.current) return
    toggleLocked.current = true
    setOpen((o) => !o)
    setTimeout(() => {
      toggleLocked.current = false
    }, NAV_TOGGLE_LOCK_MS)
  }

  /* The panel is uncovered by a clip-path, which hides it visually but leaves
     its links in the tab order — a closed panel would swallow the focus ring
     into a surface nobody can see. `inert` on the wrapper takes the whole
     subtree out of tabbing and out of the accessibility tree, so the visual
     state and the focus state can no longer disagree.

     Escape closes it and hands focus back to the control that opened it: the
     panel is a disclosure, not a modal, so it needs an escape route and a
     focus return, not a trap that would strand the visitor inside it. */
  useEffect(() => {
    if (!open) return undefined
    const onKeyDown = (event) => {
      if (event.key !== 'Escape') return
      setOpen(false)
      burgerRef.current?.focus()
    }
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
  }, [open])

  useGSAP(
    () => {
      if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return

      const header = headerRef.current
      const actions = actionsRef.current
      const cta = ctaRef.current
      const burger = burgerRef.current
      if (!header || !actions || !cta || !burger) return

      const desktop = window.matchMedia('(min-width: 768px)').matches
      if (desktop) {
        gsap.set(burger, {
          autoAlpha: 0,
          x: 8,
          scale: 0.98,
          pointerEvents: 'none',
        })
      }

      let hidden = false

      const hideHeader = () => {
        setOpen(false)
        gsap.set(header, { pointerEvents: 'none' })
        gsap.to(header, {
          yPercent: -100,
          opacity: 0,
          duration: 0.32,
          ease: 'power3.inOut',
          overwrite: 'auto',
          onComplete: () => gsap.set(header, { visibility: 'hidden' }),
        })

        if (!desktop) return

        // The burger lives in an absolute slot, so no flex reflow occurs
        // during the exchange. Only compositor-friendly transforms and
        // opacity change; Install moves left while the burger takes its slot.
        const gap = Number.parseFloat(getComputedStyle(actions).gap) || 0
        const shift = burger.offsetWidth + gap
        gsap.timeline({ defaults: { overwrite: 'auto' } })
          .to(cta, { x: -shift, duration: 0.82, ease: 'power2.inOut' })
          .to(
            burger,
            { autoAlpha: 1, x: 0, scale: 1, duration: 0.62, ease: 'power2.out', pointerEvents: 'auto' },
            0.16
          )
      }

      const showHeader = () => {
        setOpen(false)
        gsap.set(header, { visibility: 'visible', pointerEvents: 'auto' })
        gsap.to(header, {
          yPercent: 0,
          opacity: 1,
          duration: 0.42,
          ease: 'expo.out',
          overwrite: 'auto',
        })

        if (!desktop) return

        gsap.timeline({ defaults: { overwrite: 'auto' } })
          .to(cta, { x: 0, duration: 0.82, ease: 'power2.inOut' })
          .to(
            burger,
            {
              autoAlpha: 0,
              x: 8,
              scale: 0.98,
              duration: 0.52,
              ease: 'power2.in',
              pointerEvents: 'none',
            },
            0.06
          )
      }

      const st = ScrollTrigger.create({
        onUpdate: (self) => {
          const y = self.scroll()
          if (self.direction === 1 && y > CONDENSE_THRESHOLD && !hidden) {
            hidden = true
            hideHeader()
          } else if ((self.direction === -1 || y <= 0) && hidden) {
            hidden = false
            showHeader()
          }
        },
      })

      return () => st.kill()
    },
    { scope: headerRef }
  )

  return (
    <>
      <header
        ref={headerRef}
        className="fixed top-0 left-0 right-0 z-50 h-16 border-b border-night-rim/40 bg-night-void/45 backdrop-blur-xl"
      >
        <div className="mx-auto grid h-full max-w-[1200px] grid-cols-[1fr_auto_1fr] items-center px-6">
          {/* Logo */}
          <a
            href="#"
            className="flex items-center gap-2.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cloud rounded"
          >
            <img
              src="/aegis.webp"
              width="30"
              height="30"
              alt=""
              aria-hidden="true"
              className="h-[30px] w-[30px] object-contain"
              style={{ filter: 'grayscale(1) brightness(1.45) contrast(1.05)' }}
            />
            {/* Inter's caps at 16px uppercase read as a cramped block at the
                default tracking, so the wordmark carries real letterspacing —
                tracking-wide (0.025em) was too tight, 0.09em opens it up. The
                wordmark is the nav's one heading-level element, so it takes
                Inter Bold like the page's other headings. */}
            <span className="font-inter-variable text-base font-bold uppercase tracking-[0.09em] text-cloud">
              aegis
            </span>
          </a>

          {/* Nav links — desktop */}
          <nav
            className="hidden items-center justify-self-center gap-7 md:flex"
            aria-label="Main navigation"
          >
            {NAV_LINKS.map(({ label, href }) => (
              <a
                key={label}
                href={href}
                className="font-inter-variable text-caption text-cloud-dim transition-colors duration-150 hover:text-cloud focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cloud rounded-sm px-1"
              >
                {label}
              </a>
            ))}
          </nav>

        </div>
      </header>

      {/* This cluster is deliberately outside the disappearing navigation
          line: Install stays available while the burger replaces its old
          rightmost slot on desktop. */}
      <div
        ref={actionsRef}
        className="fixed top-[14px] z-50 flex items-center gap-3"
        style={{ right: 'max(1.5rem, calc((100vw - 1200px) / 2 + 1.5rem))' }}
      >
        <a
          ref={ctaRef}
          href="https://github.com/IliasAlmerekov/aegis-shellguard"
          target="_blank"
          rel="noopener noreferrer"
          className="nav-cta flex h-9 items-center gap-2 rounded-full border border-night-rim px-4 font-inter-variable text-caption text-cloud cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cloud focus-visible:ring-offset-2 focus-visible:ring-offset-night-void"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" />
          </svg>
          Install
        </a>

        <button
          ref={burgerRef}
          className={`burger relative flex h-9 w-9 items-center justify-center rounded text-cloud-dim transition-colors duration-150 hover:text-cloud focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cloud md:absolute md:right-0 md:top-0 md:opacity-0 md:pointer-events-none cursor-pointer${open ? ' is-open' : ''}`}
          onClick={toggleOpen}
          aria-label={open ? 'Close menu' : 'Open menu'}
          aria-expanded={open}
          aria-controls="mobile-nav"
        >
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
            <line className="burger-blade burger-blade--top" x1="2" y1="5" x2="16" y2="5" />
            <line className="burger-blade burger-blade--mid" x1="2" y1="9" x2="16" y2="9" />
            <line className="burger-blade burger-blade--bot" x1="2" y1="13" x2="16" y2="13" />
          </svg>
        </button>
      </div>

      {/* Opened by the mobile trigger and by the revealed desktop burger. */}
      <div
        id="mobile-nav"
        ref={panelRef}
        className={`mobile-nav-panel fixed left-0 right-0 z-40 overflow-hidden${open ? ' is-open' : ''}`}
        style={{ top: '64px' }}
        {...(open
          ? {}
          : {
              /* A boolean, not the empty string HTML itself wants: React 19
                 reflects the boolean onto the attribute, and reads an empty
                 string as false — which would leave the closed panel
                 tabbable, the exact bug `inert` is here to prevent. */
              inert: true,
              'aria-hidden': true,
            })}
      >
        <div className="slash-edge" aria-hidden="true" />
        <div className="border-b border-night-rim/40 bg-night-deep/95 backdrop-blur-md px-6 py-4">
          <nav className="flex flex-col" aria-label="Mobile navigation">
            {NAV_LINKS.map(({ label, href }, i) => (
              <a
                key={label}
                href={href}
                onClick={() => setOpen(false)}
                className="mobile-nav-link link-arrow-group flex items-center justify-between py-3.5 font-inter-variable text-sm text-cloud-dim hover:text-cloud focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-cloud rounded-sm"
                style={{
                  borderBottom: i < NAV_LINKS.length - 1 ? '1px solid rgba(54,66,77,0.5)' : 'none',
                }}
              >
                {label}
                <svg className="link-arrow" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d="M9 18l6-6-6-6" />
                </svg>
              </a>
            ))}
          </nav>
        </div>
      </div>
    </>
  )
}
