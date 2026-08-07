import { Reveal } from '../ui/Reveal'

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
// Each cell is the whole link, so the file that settles the claim is named on
// the card rather than hidden behind a second affordance.
//
// `wide` marks the two cells that hold a column pair from the two-column fold
// up — the fastest path and the honest limit, the two the page leads with.
const PILLARS = [
  {
    id: 'safe-path',
    icon: '/icons/safe.webp',
    title: 'Safe path',
    note: '1.84 ms per 1,000 commands scanned.',
    wide: true,
    source: { label: 'scanner_bench.rs', href: REPO + 'benches/scanner_bench.rs' },
  },
  {
    id: 'no-telemetry',
    icon: '/icons/telemetry.webp',
    title: 'No telemetry',
    note: 'Zero HTTP clients in the tree.',
    source: { label: 'Cargo.lock', href: REPO + 'Cargo.lock' },
  },
  {
    id: 'audit-trail-card',
    icon: '/icons/audit.webp',
    title: 'Audit trail',
    note: 'One hash-chained line per decision.',
    source: {
      label: 'ADR-017',
      href: REPO + 'docs/adr/adr-017-audit-integrity-chain-has-no-external-anchor.md',
    },
  },
  {
    id: 'intrinsic-block',
    icon: '/icons/blocks.webp',
    title: 'Intrinsic blocks',
    note: 'Seven patterns nothing bypasses.',
    source: { label: 'patterns.toml', href: REPO + 'crates/aegis-scanner/patterns.toml' },
  },
  {
    id: 'config-ratchet',
    icon: '/icons/config.webp',
    title: 'Config ratchet',
    note: '29 fields a cloned config can only tighten.',
    source: {
      label: 'ADR-013',
      href: REPO + 'docs/adr/adr-013-project-config-security-ratchet.md',
    },
  },
  {
    id: 'honest-limits',
    icon: '/icons/limits.webp',
    title: 'Honest limits',
    note: 'A guardrail on command text, not OS isolation.',
    wide: true,
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
      className="relative bg-[#000000] py-24 lg:py-32"
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

      <div className="mx-auto max-w-[1200px] px-6">
        <Reveal className="ev-intro">
          <h2 id="trust-heading" className="ev-title">
            Built to be <span className="ev-title-em">trusted</span>—six claims,
            each one a <span className="ev-title-em">line you can open in the repository.</span>
          </h2>
        </Reveal>

        <ul className="ev-bento">
          {PILLARS.map((p, i) => {
            return (
              <li
                key={p.id}
                className={`ev-cell${p.wide ? ' ev-cell--wide' : ''}`}
              >
                <Reveal delay={60 + i * 70} className="ev-cell-reveal">
                  <a
                    className="ev-card"
                    id={p.id}
                    href={p.source.href}
                    target="_blank"
                    rel="noreferrer"
                  >
                    <span className="ev-card-head">
                      <span className="ev-icon-tile" aria-hidden="true">
                        <img
                          className="ev-icon"
                          src={p.icon}
                          alt=""
                          width="384"
                          height="384"
                          loading="lazy"
                          decoding="async"
                        />
                      </span>

                      <span className="ev-source">
                        <span>{p.source.label}</span>
                        <ArrowUpRight className="ev-arrow" />
                      </span>
                    </span>

                    <span className="ev-body">
                      <span className="ev-title-card">{p.title}</span>
                      <span className="ev-note">{p.note}</span>
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
