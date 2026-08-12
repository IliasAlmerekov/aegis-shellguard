'use client'

import dynamic from 'next/dynamic'
import { Nav } from '@/components/ui/Nav'
import { HeroDarkMatter } from '@/components/sections/HeroDarkMatter'
import { Footer } from '@/components/sections/Footer'

// Below-the-fold sections are code-split: the hero is the LCP surface and
// owns the critical path, so every section past it is fetched on demand as
// the visitor scrolls toward it. `lib/gsap` is in the shared chunk (the hero
// imports it eagerly for its pin), so a split section that uses GSAP reuses
// the loaded module rather than re-fetching it.
//
// `next/dynamic` rather than `React.lazy`, which this replaced: both split
// the chunk, but only the former still renders the section during the static
// export. Under `React.lazy` every section below the hero resolved to the
// Suspense fallback at build time and existed in the shipped HTML as nothing
// at all — the SPA could afford that, a landing page being read by crawlers
// cannot.
const WhyAegis = dynamic(() =>
  import('@/components/sections/WhyAegis').then((m) => m.WhyAegis)
)
const GateDemo = dynamic(() =>
  import('@/components/sections/GateDemo').then((m) => m.GateDemo)
)
const HowItWorks = dynamic(() =>
  import('@/components/sections/HowItWorks').then((m) => m.HowItWorks)
)
const IncidentLog = dynamic(() =>
  import('@/components/sections/IncidentLog').then((m) => m.IncidentLog)
)
const Evidence = dynamic(() =>
  import('@/components/sections/Evidence').then((m) => m.Evidence)
)
const CTABanner = dynamic(() =>
  import('@/components/sections/CTABanner').then((m) => m.CTABanner)
)

function Divider() {
  return (
    <div aria-hidden="true" className="mx-auto max-w-[1200px] px-gutter">
      <div className="h-px bg-night-edge" />
    </div>
  )
}

export default function Page() {
  return (
    <div className="min-h-dvh bg-[#000000] text-cloud-dim">
      {/* First thing in the tab order, and invisible until it holds focus.
          Without it a keyboard visitor pays the wordmark, three nav links,
          Install and the burger on every arrival before reaching any content
          — and the nav is fixed, so it is re-entered on the way back too. */}
      <a href="#main" className="skip-link">
        Skip to content
      </a>
      <Nav />
      <main id="main" tabIndex={-1}>
        <HeroDarkMatter />
        {/* No rule between the hero and the claim: the shot ends on black and
            the type stands on the same black, so the two read as one fall
            rather than two panels. The gate then shows what the claim just
            said, so it follows without a rule between them either. */}
        <WhyAegis />
        <GateDemo />
        <Divider />
        <HowItWorks />
        <IncidentLog />
        <Evidence />
        <Divider />
        <CTABanner />
      </main>
      <Footer />
    </div>
  )
}
