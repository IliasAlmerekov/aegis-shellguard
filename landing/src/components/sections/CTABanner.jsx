import { Reveal } from '../ui/Reveal'

export function CTABanner() {
  return (
    <section
      className="taped-footer-section"
      aria-labelledby="get-started-heading"
    >
      <Reveal>
        <div className="taped-footer-card">
          <span className="taped-footer-tape taped-footer-tape--left" aria-hidden="true" />
          <span className="taped-footer-tape taped-footer-tape--right" aria-hidden="true" />

          <div className="taped-footer-layout">
            <div className="taped-footer-brand">
              <a href="/" className="taped-footer-wordmark" aria-label="Aegis home">
                <img
                  src="/aegis.webp"
                  width="30"
                  height="30"
                  alt=""
                  aria-hidden="true"
                />
                <span>Aegis</span>
              </a>

              <p>
                Open source, zero telemetry, minimal overhead on the safe path.
                Install with the installer, Homebrew, npm, or Cargo and your AI
                agents work under supervision.
              </p>
            </div>

            <div className="taped-footer-cta">
              <h2 id="get-started-heading">Guard your stack in minutes.</h2>
              <div className="taped-footer-actions">
                <a
                  href="https://github.com/IliasAlmerekov/aegis-shellguard"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="taped-footer-primary"
                >
                  View on GitHub
                </a>
                {/* "Read the docs" pointed at #how-it-works — a section of
                    this page, three steps long, that the visitor has already
                    scrolled past to reach this card. The label was the honest
                    one; the target was not, so the target moves. */}
                <a
                  href="https://github.com/IliasAlmerekov/aegis-shellguard#readme"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="taped-footer-secondary"
                >
                  Read the docs
                </a>
              </div>
            </div>
          </div>
        </div>
      </Reveal>
    </section>
  )
}
