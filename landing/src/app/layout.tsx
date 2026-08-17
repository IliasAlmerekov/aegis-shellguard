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
  HERO DIRECTION CONTRACT — see DESIGN.md for the reasoning and
  src/lib/scene/config.ts for every number.

  THESIS: the guardrail is a held hand, not a shield. Refuses the
    product-screenshot hero, and refuses the floating 3D mark just as much:
    the object is mass that holds, not a logo that spins.
  OWN-WORLD: the brand palette split 60/30/10 — Night Black as substrate
    (the page canvas is pure #000, which the hero's own gradients terminate
    on so the shot meets the section below without a seam), Cloud White for
    the headline over dimmer copy, and the accent held to two jobs:
    Electric Blue is the one action, Cyan Neon is live state — including the
    light inside the stone, which is the same two colours and no others.
    Hairline edges, 6px radii, no ornament. Recognizable with all copy
    removed.
  STORY: an agent reaches for something destructive and the stone opens along
    its fractures — then the guardrail snaps it shut. Approval is the product,
    and the visitor installs it.
  FIRST VIEWPORT: a fractured stone cube hanging slightly above centre in a
    lit blue field, balancing rather than rotating; centred headline, one
    primary action and the install snippet beneath it; a vignette to the
    bottom of the frame carrying the headline's contrast.
  FORM: a WebGL scene (react-three-fiber), pinned 200vh and scrubbed. One
    channel between scroll and scene — pin progress through a density map —
    drives the camera approach, the copy flying past on that same phase, the
    opening, the Policy Lock and the handoff to black. Quality descends a
    one-way ladder measured in frame time; under `prefers-reduced-motion`
    there is no pin at all and the scene renders one still frame.
  FINISH: unreviewed and undocumented is unfinished; this build ends
    with the finish review, the verdict, and DESIGN.md.

  This replaced a scroll-scrubbed 161-frame canvas sequence. That hero, its
  8.7 MB of frames and its `FrameSequence` component are gone from the tree
  and live in the history, which is where they will be recovered from if they
  are ever wanted again.
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
        {/* The scene is an above-the-fold LCP companion. Starting these three
            same-origin requests in the document head removes the hydration and
            dynamic-import wait before TextureLoader can ask for them.

            `crossOrigin` is not decoration here, and its absence cost the page
            every byte this preload was meant to save. Three's ImageLoader sets
            `crossOrigin = 'anonymous'` on the element it creates; a preload
            without the attribute is a different CORS mode, so the browser
            refuses to hand the warm entry over and fetches the file a second
            time. Measured: 1.6 MB of textures on a phone for 805 KB of
            distinct bytes — every map downloaded exactly twice.

            The normal map is the one that ships in two sizes (see
            `StoneCube.tsx`), so its preload is split by the same query the
            scene picks with. `media` on a preload is honoured, and only the
            matching one is fetched. */}
        <link
          rel="preload"
          href="/textures/rock/albedo.webp"
          as="image"
          type="image/webp"
          crossOrigin="anonymous"
        />
        <link
          rel="preload"
          href="/textures/rock/normal.webp"
          as="image"
          type="image/webp"
          crossOrigin="anonymous"
          media="(min-width: 768px)"
        />
        <link
          rel="preload"
          href="/textures/rock/normal-512.webp"
          as="image"
          type="image/webp"
          crossOrigin="anonymous"
          media="(max-width: 767.98px)"
        />
        <link
          rel="preload"
          href="/textures/rock/orm.webp"
          as="image"
          type="image/webp"
          crossOrigin="anonymous"
        />
        <link
          rel="preload"
          href="/hdri/studio-small-08-512.hdr"
          as="fetch"
          type="application/octet-stream"
          crossOrigin="anonymous"
        />
        {/* The sequence's first frame is no longer preloaded here. The WebGL
            scene replaced the sequence, and for a while the preload outlived
            the replacement — the browser pulled that frame at the highest
            priority on every load, into the very window in which the stone's
            maps have to arrive before first paint.

            The sequence itself went with the preload: 8.7 MB under `public`
            (`frames`, `frames-sm`) and the `FrameSequence` component, which
            nothing referenced. A dead hero is not kept around "just in case";
            it lives in the history, which is where it will be recovered from
            if it is ever wanted. */}
        {children}
      </body>
    </html>
  )
}
