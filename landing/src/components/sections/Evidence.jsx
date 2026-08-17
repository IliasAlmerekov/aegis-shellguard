import { Reveal } from '../ui/Reveal'
import {
  FigAuditTrail,
  FigConfigRatchet,
  FigHonestLimits,
  FigIntrinsicBlocks,
  FigNoTelemetry,
  FigSafePath,
} from './EvidenceFigures'

/* Inline rather than pulled from an icon package: it is the only glyph on the
   page that is not one of the marks below, and the project ships no icon
   dependency. */
function ArrowUpRight({ className }) {
  return (
    <svg
      className={className}
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M7 17 17 7" />
      <path d="M7 7h10v10" />
    </svg>
  )
}

const REPO = 'https://github.com/IliasAlmerekov/aegis-shellguard/blob/main/'

// Every figure below is read off the repository, not written for the page:
// the bench number, a grep that returns nothing, the count of Block-level
// built-ins, the count of fields the project ratchet covers, the ADR number.
// Each cell is the whole link, so the file that settles the claim is named in
// the column rather than hidden behind a second affordance.
//
// `fig` is the plate number, and it is not decoration: the section's whole
// conceit is that these are figures a reader can go and check, so each one is
// numbered the way a figure in a paper is, and the number is what the drawing
// above it is captioned by.
const PILLARS = [
  {
    id: 'safe-path',
    fig: 'FIG 0.1',
    Figure: FigSafePath,
    title: 'Safe path',
    note: '1.84 ms per 1,000 commands scanned.',
    source: { label: 'scanner_bench.rs', href: REPO + 'benches/scanner_bench.rs' },
  },
  {
    id: 'no-telemetry',
    fig: 'FIG 0.2',
    Figure: FigNoTelemetry,
    title: 'No telemetry',
    note: 'Zero HTTP clients in the tree.',
    source: { label: 'Cargo.lock', href: REPO + 'Cargo.lock' },
  },
  {
    id: 'audit-trail-card',
    fig: 'FIG 0.3',
    Figure: FigAuditTrail,
    title: 'Audit trail',
    note: 'One hash-chained line per decision.',
    source: {
      label: 'ADR-017',
      href: REPO + 'docs/adr/adr-017-audit-integrity-chain-has-no-external-anchor.md',
    },
  },
  {
    id: 'intrinsic-block',
    fig: 'FIG 0.4',
    Figure: FigIntrinsicBlocks,
    title: 'Intrinsic blocks',
    note: 'Seven patterns nothing bypasses.',
    source: { label: 'patterns.toml', href: REPO + 'crates/aegis-scanner/patterns.toml' },
  },
  {
    id: 'config-ratchet',
    fig: 'FIG 0.5',
    Figure: FigConfigRatchet,
    title: 'Config ratchet',
    note: '29 fields a cloned config can only tighten.',
    source: {
      label: 'ADR-013',
      href: REPO + 'docs/adr/adr-013-project-config-security-ratchet.md',
    },
  },
  {
    id: 'honest-limits',
    fig: 'FIG 0.6',
    Figure: FigHonestLimits,
    title: 'Honest limits',
    note: 'A guardrail on command text, not OS isolation.',
    source: {
      label: 'ADR-003',
      href: REPO + 'docs/adr/adr-003-aegis-is-a-heuristic-guardrail-not-a-sandbox.md',
    },
  },
]

export function Evidence() {
  return (
    <section
      id="trust"
      aria-labelledby="trust-heading"
      className="relative bg-[#000000] py-section md:py-section-lg"
    >
      {/* Kept so any existing link to #audit-trail still lands on the section
          that carries the claim. The clearance for the fixed nav is the root's
          `scroll-padding-top` now, so the marker sits on the section's own top
          edge rather than carrying a second offset that would stack with it. */}
      <span
        id="audit-trail"
        aria-hidden="true"
        className="pointer-events-none absolute top-0 left-0 h-px w-px"
      />

      <div className="mx-auto max-w-[1200px] px-gutter">
        <Reveal className="ev-intro">
          <h2 id="trust-heading" className="ev-title">
            Built to be <span className="ev-title-em">trusted</span>—six claims,
            each one a <span className="ev-title-em">line you can open in the repository.</span>
          </h2>
        </Reveal>

        {/* A plate table, not a card wall. The hairline between two columns
            is the only edge in the section: nothing is boxed, so the drawings
            sit on the page's own black and the eye groups them by the rules
            and the shared baselines instead of by six containers. */}
        <ul className="ev-plates">
          {PILLARS.map((p, i) => {
            const { Figure } = p
            return (
              <li key={p.id} className="ev-plate">
                <Reveal delay={60 + (i % 3) * 70} className="ev-plate-reveal">
                  <a
                    className="ev-figure"
                    id={p.id}
                    href={p.source.href}
                    target="_blank"
                    rel="noreferrer"
                  >
                    <span className="ev-fig-no">{p.fig}</span>

                    <span className="ev-fig-art" aria-hidden="true">
                      <Figure />
                    </span>

                    <span className="ev-caption">
                      <span className="ev-title-card">{p.title}</span>
                      <span className="ev-note">{p.note}</span>
                      <span className="ev-source">
                        <span>{p.source.label}</span>
                        <ArrowUpRight className="ev-arrow" />
                      </span>
                    </span>

                    <span className="sr-only">
                      {' '}— verified in {p.source.label}
                    </span>
                  </a>
                </Reveal>
              </li>
            )
          })}
        </ul>
      </div>
    </section>
  )
}
