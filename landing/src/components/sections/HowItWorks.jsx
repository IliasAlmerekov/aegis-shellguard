import { useRef, useState } from 'react'
import { gsap, ScrollTrigger, useGSAP } from '../../lib/gsap'
import { Reveal } from '../ui/Reveal'

/* How it works — one session, three steps.
   The terminal is a scrollback that accretes: as the visitor scrolls through
   the three steps, each step's lines appear in the terminal and stay, the way
   a real scrollback behaves. Nothing already read is taken away — the same
   rule the gate demo runs on. The terminal is sticky on desktop so the session
   stays in view while the steps scroll past it; on a phone each step carries
   its own inline fragment, because a sticky surface there would eat the
   screen the captions need.

   Palette and chrome match the gate demo — the two surfaces are one field, not
   two themed panels. No eyebrow: the heading carries the section, the way the
   migrated surfaces below the hero already do. */

/* `held` is the only chromatic note in the section — it has to read at 11px
   against the terminal, so it takes the blue the gate demo uses for the same
   state. */
const NOTE_HELD = 'var(--color-electric)'

const STEPS = [
  {
    num: '01',
    heading: 'Install Aegis',
    body: 'Installer, Homebrew, npm, or Cargo. Package-manager installs ship a binary — no Rust toolchain needed.',
    held: false,
    lines: [
      { who: 'you', text: 'npm i -g @iliasalmerekov/aegis' },
      { mark: '+', text: '@iliasalmerekov/aegis@0.6.3' },
    ],
  },
  {
    num: '02',
    heading: 'Opt in to shell-proxy mode',
    body: 'Run setup-shell when you want tools that use $SHELL -c to route through Aegis.',
    held: false,
    lines: [
      { who: 'you', text: 'aegis setup-shell' },
      { mark: 'check', text: 'managed shell block installed' },
      { text: 'commands via $SHELL -c now route through aegis', sub: true },
    ],
  },
  {
    num: '03',
    heading: 'Approve or deny in context',
    body: 'A dangerous pattern fires. Aegis pauses on the command, the risk ID, and a safer alternative — one keystroke decides.',
    held: true,
    lines: [{ who: 'agent', text: 'DROP TABLE users;', note: 'held' }],
    panel: {
      title: 'Aegis stopped this command.',
      sub: 'DB-001 — the table is gone for good.',
      alt: 'safe alt: ALTER TABLE users RENAME TO users_retired',
    },
  },
]

/* The shield glyph the gate demo puts on its titlebar, reused so the two
   terminals read as the same window. */
function Glyph() {
  return (
    <svg width="15" height="15" viewBox="0 0 17 17" fill="none" aria-hidden="true">
      <path
        d="M8.5 1.5 14.5 4.8v7.4L8.5 15.5 2.5 12.2V4.8L8.5 1.5Z"
        className="stroke-cloud-mute"
      />
      <path d="M8.5 5.1v6.8M5.7 6.6 8.5 5l2.8 1.6" className="stroke-cloud" />
    </svg>
  )
}

/* The success mark used to be the literal "✓" (U+2713), which sits outside
   the Latin subset every face on this page is cut to — so it fell through to
   whatever the OS offered and broke the 7ch prompt column's width. Drawn, it
   is the same mark everywhere and inherits the row's colour and size.
   Mirrors the gate demo's CheckMark exactly. */
function CheckMark() {
  return (
    <svg
      width="0.75em"
      height="0.75em"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="3"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className="inline-block align-[-0.02em]"
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  )
}

/* Prompts and result marks share one fixed column, so every command and every
   line Aegis prints starts at the same x — the way a real prompt behaves.
   Mirrors the gate demo's Gutter exactly. */
function Gutter({ who, mark }) {
  if (who) {
    return (
      <span className="w-[7ch] shrink-0 select-none text-cloud-dim">{who} $</span>
    )
  }
  if (mark) {
    return (
      <span className="w-[7ch] shrink-0 select-none">
        <span className="pl-[3ch] text-cloud">
          {mark === 'check' ? <CheckMark /> : mark}
        </span>
      </span>
    )
  }
  return <span className="w-[7ch] shrink-0 select-none" />
}

function Line({ line }) {
  if (line.who) {
    return (
      <div data-line className="flex items-baseline gap-3">
        <Gutter who={line.who} />
        <span className="min-w-0 flex-1 break-all text-cloud">
          {line.text}
          {line.note && (
            <span
              className="ml-3 shrink-0 text-[11px] tracking-[0.08em] uppercase md:text-xs"
              style={{ color: NOTE_HELD }}
            >
              {line.note}
            </span>
          )}
        </span>
      </div>
    )
  }
  return (
    <div data-line className="flex items-baseline gap-3">
      <Gutter mark={line.mark} />
      <span
        className={`min-w-0 flex-1 break-all ${
          line.sub ? 'text-cloud-dim text-[12px] md:text-[13px]' : 'text-cloud'
        }`}
      >
        {line.text}
      </span>
    </div>
  )
}

/* The gate's own panel — the one moment the section is selling. Same border
   and wash as the gate demo so the held state reads the same in both places. */
function GatePanel({ panel }) {
  return (
    <div
      data-panel
      className="rounded-md border border-electric/50 bg-night/55 px-4 py-4 md:px-5"
    >
      <p className="text-[13px] text-cloud sm:text-[15px] md:text-base">{panel.title}</p>
      <p className="mt-1.5 text-[12px] text-cloud-dim sm:text-[13px] md:text-sm">{panel.sub}</p>
      <p className="mt-2 font-berkeley-mono text-[12px] text-cloud-dim sm:text-[13px] md:text-sm">
        {panel.alt}
      </p>
      <div className="mt-4 flex items-center gap-2.5">
        <span className="rounded-sm border border-night-rim px-2.5 py-1.5 text-[11px] text-cloud md:text-xs">
          Y&nbsp; allow
        </span>
        <span className="rounded-sm border border-night-rim px-2.5 py-1.5 text-[11px] text-cloud-dim md:text-xs">
          N&nbsp; deny
        </span>
      </div>
    </div>
  )
}

/* One terminal window. The status word is the only thing in the chrome that
   moves, and it moves for one reason: a command is waiting on a human. */
function TerminalChrome({ held, children }) {
  return (
    <div className="overflow-hidden rounded-xl border border-night-rim/80 bg-night-void shadow-[0_28px_80px_rgba(0,0,0,0.55)]">
      <div className="flex items-center justify-between border-b border-night-rim/60 px-4 py-3 md:px-5">
        <div className="flex items-center gap-2.5">
          <Glyph />
          <span className="font-berkeley-mono text-[11px] tracking-[-0.01em] text-cloud-dim md:text-xs">
            aegis — zsh
          </span>
        </div>
        <div className="flex items-center gap-2 font-berkeley-mono text-[11px] tracking-[0.07em] uppercase md:text-xs">
          <span
            className="h-1.5 w-1.5 rounded-full transition-colors duration-300"
            style={{
              backgroundColor: held
                ? 'var(--color-electric)'
                : 'var(--color-cyan-neon)',
            }}
          />
          <span
            className="transition-colors duration-300"
            style={{ color: held ? 'var(--color-cloud)' : 'var(--color-cloud-dim)' }}
          >
            {held ? 'waiting for you' : 'shell guarded'}
          </span>
        </div>
      </div>
      <div className="px-4 py-5 sm:px-6 md:px-7 md:py-7">{children}</div>
    </div>
  )
}

export function HowItWorks() {
  const rootRef = useRef(null)
  const stepsRef = useRef(null)
  const [active, setActive] = useState(0)
  // Mirror of `active` read inside the per-frame GSAP callback. Comparing
  // against the ref lets us skip setActive entirely on frames where the
  // index has not changed, instead of queueing an updater every tick and
  // relying on React's bail-out. The state still drives the render — the
  // ref is transient, only the last-set value.
  const activeIdxRef = useRef(0)

  useGSAP(
    () => {
      const mm = gsap.matchMedia()

      /* Reduced motion gets the session already composed — every line and the
         gate panel in place, the last step active. Nothing moves. */
      mm.add('(prefers-reduced-motion: reduce)', () => {
        gsap.set('[data-terminal] [data-line]', { opacity: 1, y: 0, filter: 'none' })
        gsap.set('[data-terminal] [data-panel]', {
          opacity: 1,
          y: 0,
          filter: 'none',
          clipPath: 'none',
        })
        setActive(2)
        activeIdxRef.current = 2
      })

      mm.add(
        {
          motion: '(prefers-reduced-motion: no-preference)',
          isDesktop: '(min-width: 768px)',
        },
        (ctx) => {
          if (!ctx.conditions.motion) return
          if (!ctx.conditions.isDesktop) return

          /* Hidden here rather than in CSS so the session still renders if the
             bundle never arrives. The mobile fragments carry the same
             data-line / data-panel attributes, so the desktop selectors are
             scoped to the sticky terminal to leave them untouched. */
          gsap.set('[data-terminal] [data-line]', { opacity: 0, y: 8, filter: 'blur(6px)' })
          gsap.set('[data-terminal] [data-panel]', {
            opacity: 0,
            y: 8,
            filter: 'blur(8px)',
            clipPath: 'inset(0 0 100% 0)',
          })

          /* Scroll only decides *when*; the timeline owns its own tempo, so
             the session reads at a speed the visitor cannot scrub backwards
             through mid-line. Each step's lines accrete and stay — nothing
             already read is taken away, the way a real scrollback behaves. */
          const tl = gsap.timeline({
            defaults: { ease: 'expo.out' },
            scrollTrigger: {
              trigger: stepsRef.current,
              start: 'top 65%',
              end: 'bottom 65%',
              scrub: 0.4,
            },
            onUpdate: () => {
              const i = Math.min(2, Math.floor(tl.progress() * 3 + 1e-6))
              if (i !== activeIdxRef.current) {
                activeIdxRef.current = i
                setActive(i)
              }
            },
          })

          STEPS.forEach((step, i) => {
            const sel = `[data-terminal] [data-group="${i}"]`
            tl.to(
              `${sel} [data-line]`,
              { opacity: 1, y: 0, filter: 'blur(0px)', duration: 0.6, stagger: 0.12 },
              i * 1.0
            )
            if (step.panel) {
              tl.to(
                `${sel} [data-panel]`,
                {
                  opacity: 1,
                  y: 0,
                  filter: 'blur(0px)',
                  clipPath: 'inset(0 0 0% 0)',
                  duration: 0.55,
                },
                i * 1.0 + 0.5
              )
            }
          })

          /* The pin's length is in vh, but the step blocks' height is not — a
             late font swap would move the start position under the scrub. */
          document.fonts?.ready.then(() => ScrollTrigger.refresh())
        }
      )

      return () => mm.revert()
    },
    { scope: rootRef }
  )

  /* Clicking a step scrolls its block into view; the scrub takes it from
     there. Keystrokes are not invented — the step explains itself. */
  const jumpTo = (i) => {
    const el = stepsRef.current?.querySelector(`[data-step="${i}"]`)
    /* An explicit `behavior` wins over the root's `scroll-behavior`, so the
       reduced-motion rule in CSS cannot reach this call — the preference has
       to be read here or a visitor who asked for less motion still gets the
       page gliding two viewports under them. */
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    el?.scrollIntoView({ block: 'center', behavior: reduce ? 'auto' : 'smooth' })
  }

  return (
    <section
      id="how-it-works"
      ref={rootRef}
      aria-labelledby="how-it-works-heading"
      className="py-24 md:py-32"
    >
      <div className="mx-auto w-full max-w-[1200px] px-6">
        <Reveal>
          <h2
            id="how-it-works-heading"
            className="max-w-[16ch] font-inter-variable text-[32px] font-bold leading-[1.1] tracking-[-0.5px] text-cloud sm:text-heading-sm sm:tracking-heading-sm md:text-heading md:tracking-heading"
          >
            One session, three steps.
          </h2>
        </Reveal>

        {/* Desktop: the steps scroll, the terminal sticks and accretes.
            Mobile: each step carries its own inline terminal fragment. */}
        <div className="mt-14 lg:mt-20 lg:grid lg:grid-cols-[minmax(0,1fr)_minmax(0,520px)] lg:items-start lg:gap-16">
          <div ref={stepsRef} className="flex flex-col">
            {STEPS.map((step, i) => {
              const isActive = i === active
              return (
                <div
                  key={step.num}
                  data-step={i}
                  className="relative py-8 lg:min-h-[42vh] lg:flex lg:items-center"
                >
                  <Reveal delay={i * 60}>
                    <button
                      type="button"
                      onClick={() => jumpTo(i)}
                      aria-current={isActive ? 'step' : undefined}
                      className="group relative w-full cursor-pointer pl-5 text-left focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-cloud rounded"
                    >
                      {/* A 1px hairline that brightens on the active step — the
                          rail motif the hero and the why-aegis rule already
                          use, kept to 1px so it reads as structure, not chrome. */}
                      <span
                        aria-hidden="true"
                        className="absolute left-0 top-0 h-full w-px transition-colors duration-300"
                        style={{
                          backgroundColor: isActive
                            ? 'var(--color-cyan-neon)'
                            : 'var(--color-night-edge)',
                        }}
                      />
                      {/* The resting number was `night-edge`, the hairline
                          colour — 1.6:1 on black, which is a rule pretending
                          to be a numeral. It is content, not chrome, so it
                          takes the palette's dimmest *text* step: 5.8:1 at
                          40px, and still visibly a step behind the active
                          number's white. */}
                      <span
                        className="block font-inter-variable text-[40px] font-bold leading-none transition-colors duration-300 lg:text-[44px]"
                        style={{
                          color: isActive
                            ? 'var(--color-cloud)'
                            : 'var(--color-cloud-faint)',
                        }}
                      >
                        {step.num}
                      </span>
                      <span
                        className="mt-3 block font-inter-variable text-lg font-bold leading-snug transition-colors duration-300 sm:text-xl"
                        style={{
                          color: isActive
                            ? 'var(--color-cloud)'
                            : 'var(--color-cloud-mute)',
                        }}
                      >
                        {step.heading}
                      </span>
                      <span
                        className="mt-2 block max-w-[42ch] font-inter-variable text-body-sm leading-body-sm tracking-body-sm transition-opacity duration-300 md:text-base md:leading-[1.6]"
                        style={{ color: 'var(--color-cloud-dim)', opacity: isActive ? 1 : 0.65 }}
                      >
                        {step.body}
                      </span>
                    </button>
                  </Reveal>

                  {/* Mobile: the step's own terminal fragment, inline. */}
                  <div className="mt-6 lg:hidden">
                    <Reveal delay={i * 60 + 120}>
                      <TerminalChrome held={step.held}>
                        <div className="space-y-2.5 font-berkeley-mono text-[13px] leading-[1.6] sm:text-[15px]">
                          {step.lines.map((line, k) => (
                            <Line key={k} line={line} />
                          ))}
                          {step.panel && <GatePanel panel={step.panel} />}
                        </div>
                      </TerminalChrome>
                    </Reveal>
                  </div>
                </div>
              )
            })}
          </div>

          {/* Desktop: one sticky terminal that accretes the whole session as
              the steps scroll past it. All lines render in flow so the window
              is full-height from the first frame — accretion changes opacity,
              not layout, so nothing shifts as lines land. */}
          <div data-terminal className="hidden lg:sticky lg:top-24 lg:block lg:self-start">
            <TerminalChrome held={STEPS[active]?.held ?? false}>
              <div className="space-y-2.5 font-berkeley-mono text-[13px] leading-[1.6] sm:text-[15px]">
                {STEPS.map((step, i) => (
                  <div key={i} data-group={i} className="space-y-2.5">
                    {step.lines.map((line, k) => (
                      <Line key={k} line={line} />
                    ))}
                    {step.panel && <GatePanel panel={step.panel} />}
                  </div>
                ))}
              </div>
            </TerminalChrome>
          </div>
        </div>
      </div>
    </section>
  )
}