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
  HERO DIRECTION CONTRACT — see PLAN.md for the scene, DESIGN.md for the page.
  THESIS: the guardrail watches. Dark organic matter hangs in the frame and
    something inside it is looking back — the product's argument before a
    word of copy is read. Refuses the product screenshot and the floating
    logo alike.
  OWN-WORLD: the brand four, split 60/30/10 — 60% Night Black (#1e272e down
    to #04090e as the ground the matter floats on), 30% Cloud White (#f5f6fa
    headline over #93a1ae/#aeb8c4 copy), 10% accent held to two jobs:
    Electric Blue #0984e3 is the one action, Cyan Neon #00cec9 is live state.
    In the scene the same accents arrive as light through a membrane rather
    than as paint on it. #27333c hairline edges, 6px radii, no ornament.
  STORY: the visitor meets something alive and attentive, then reads what it
    does, then installs it.
  FIRST VIEWPORT: the matter in the right half with negative space around it,
    72px headline and the primary action on clean black at the 1200px gutter.
  FORM: a raymarched signed distance field on a full-screen quad — one custom
    shader, no models — pinned two viewport heights, the camera closing on
    the matter and passing through the particle cloud beside it.
  FINISH: unreviewed and undocumented is unfinished.
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
        {children}
      </body>
    </html>
  )
}
