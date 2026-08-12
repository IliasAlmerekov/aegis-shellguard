import type { NextConfig } from 'next'

const nextConfig: NextConfig = {
  /* DigitalOcean App Platform serves this as a Static Site component, so
     there is no Node process at request time. `export` emits the whole page
     as files under `out/`, which is what the platform's build settings point
     at. Anything that needs a server — middleware, route handlers, image
     optimisation on demand — is unavailable by construction, and that is the
     intended trade: a landing page with no server-side data has nothing to
     gain from a runtime it would have to keep alive and pay for. */
  output: 'export',

  /* Next 16 writes its own AGENTS.md and CLAUDE.md into the project root on
     first run. This repository already carries agent instructions — the
     workspace CLAUDE.md and CONVENTION.md at the root above — and a
     generated file with the same name shadowing them is worse than no file
     at all. */
  agentRules: false,

  images: {
    /* The optimiser is a server feature and `output: 'export'` has no server.
       Every image on this page is a hand-tuned webp already sized for its
       slot, so there is nothing for the optimiser to do that the assets do
       not already do for themselves. */
    unoptimized: true,
  },
}

export default nextConfig
