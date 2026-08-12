import type { Metadata, Viewport } from 'next'
import './globals.css'

export const metadata: Metadata = {
  title: 'Aegis ShellGuard — stop your AI agent from deleting your work',
  description:
    'Aegis ShellGuard is a Rust shell guardrail for AI coding agents. rm -rf, git reset --hard and DROP TABLE wait for your approval; safe commands pass through in under 2 ms.',
  icons: { icon: '/favicon.ico' },
}

export const viewport: Viewport = {
  /* The page has one appearance and it is dark. Declaring the scheme rather
     than letting the browser infer it stops the flash of a light form-control
     chrome before the stylesheet lands. */
  colorScheme: 'dark',
  themeColor: '#000000',
}

/*
  HERO DIRECTION CONTRACT — see DESIGN.md
  THESIS: the guardrail is a held hand, not a shield. Refuses the
    product-screenshot hero and the floating 3D mark it replaces.
  OWN-WORLD: the brand four, split 60/30/10 — 60% Night Black (#1e272e,
    darkened to #04090e as the substrate the footage fades into), 30%
    Cloud White (#f5f6fa headline over #93a1ae/#aeb8c4 copy), 10% accent
    held to two jobs: Electric Blue #0984e3 is the one action, Cyan Neon
    #00cec9 is live state (the scroll rail, the shell prompt). #27333c
    hairline edges, 6px radii, no ornament. Recognizable with all copy
    removed.
  STORY: an agent reaches for something destructive and stops short —
    so approval is the product, and the visitor installs it.
  FIRST VIEWPORT: full-bleed frame 001 of the shot, subject held in the
    right third and its own headroom leaving air under the nav; graded
    substrate scrim on the left carrying a 72px headline and the primary
    action at the 1200px gutter; hairline scroll rail at the content
    gutter.
  FORM: scroll-scrubbed 161-frame canvas sequence, pinned 150vh, text
    accreting in three beats.
  FINISH: unreviewed and undocumented is unfinished; this build ends
    with the finish review, the verdict, and DESIGN.md.

  PLAN.md describes a Dark Matter scene intended to replace this hero. It was
  built and reverted; the contract above is the one the page actually runs
  against.
*/

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    /* No inline style on either tag: the ground is set in globals.css, and
       `margin: 0` is already Tailwind's preflight. Inline styles here cost a
       hydration mismatch for nothing — the browser rewrites them into
       longhand and normalised colour before React can compare its tree. */
    <html lang="en">
      <body>
        {/* Type is self-hosted: no third party on the critical render path.
            Inter and JetBrains Mono carry the hero (headline, nav wordmark,
            install snippet), so both are preloaded — the swap lands before
            first paint instead of reflowing the hero. Shadows Into Light is
            the brand-signature accent on the hero eyebrow and the footer
            wordmark; it is self-hosted but not preloaded (a single short
            label, off the critical path).

            React hoists these into <head> wherever they are rendered, which
            is why they can sit here rather than in a <head> tag the App
            Router does not accept. */}
        <link
          rel="preload"
          href="/fonts/inter-latin-variable.woff2"
          as="font"
          type="font/woff2"
          crossOrigin="anonymous"
        />
        <link
          rel="preload"
          href="/fonts/jetbrains-mono-latin-variable.woff2"
          as="font"
          type="font/woff2"
          crossOrigin="anonymous"
        />
        {/* The hand is at the bottom of frame 1 and the headline reads
            against it, so the first frame is fetched at the same priority as
            the type rather than after the JS bundle resolves. `media` keeps
            each viewport from paying for the other's frame set. */}
        <link
          rel="preload"
          href="/frames/frame-0001.webp"
          as="image"
          type="image/webp"
          media="(min-width: 768px)"
        />
        <link
          rel="preload"
          href="/frames-sm/frame-0001.webp"
          as="image"
          type="image/webp"
          media="(max-width: 767px)"
        />
        {children}
      </body>
    </html>
  )
}
