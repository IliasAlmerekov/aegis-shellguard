import { lazy, Suspense } from 'react'
import { Nav } from './components/ui/Nav'
import { Hero } from './components/sections/Hero'
import { Footer } from './components/sections/Footer'

// Below-the-fold sections are code-split: the hero is the LCP surface and
// owns the critical path, so every section past it is fetched on demand as
// the visitor scrolls toward it. `lib/gsap` is already in the entry chunk
// (main.jsx imports it eagerly for the hero's pin), so a lazy section that
// uses GSAP reuses the loaded module rather than re-fetching it.
const WhyAegis = lazy(() =>
  import('./components/sections/WhyAegis').then((m) => ({ default: m.WhyAegis }))
)
const GateDemo = lazy(() =>
  import('./components/sections/GateDemo').then((m) => ({ default: m.GateDemo }))
)
const HowItWorks = lazy(() =>
  import('./components/sections/HowItWorks').then((m) => ({ default: m.HowItWorks }))
)
const IncidentLog = lazy(() =>
  import('./components/sections/IncidentLog').then((m) => ({ default: m.IncidentLog }))
)
const Evidence = lazy(() =>
  import('./components/sections/Evidence').then((m) => ({ default: m.Evidence }))
)
const CTABanner = lazy(() =>
  import('./components/sections/CTABanner').then((m) => ({ default: m.CTABanner }))
)

function Divider() {
  return (
    <div
      aria-hidden="true"
      className="mx-auto max-w-[1200px] px-6"
    >
      <div className="h-px bg-night-edge" />
    </div>
  )
}

export default function App() {
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
        <Hero />
        {/* No rule between the hero and the claim: the shot ends on black and
            the type stands on the same black, so the two read as one fall
            rather than two panels. The gate then shows what the claim just
            said, so it follows without a rule between them either. */}
        <Suspense fallback={null}>
          <WhyAegis />
          <GateDemo />
          <Divider />
          <HowItWorks />
          <IncidentLog />
          <Evidence />
          <Divider />
          <CTABanner />
        </Suspense>
      </main>
      <Footer />
    </div>
  )
}