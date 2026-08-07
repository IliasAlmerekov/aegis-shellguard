import gsap from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'
import { useGSAP } from '@gsap/react'

// Register GSAP plugins exactly once for the whole app. Importing this
// module is the single side effect; every consumer re-exports from here so a
// lazy-loaded section can never load before its plugins are registered
// (import order would otherwise decide whether ScrollTrigger exists).
gsap.registerPlugin(useGSAP, ScrollTrigger)

export { gsap, ScrollTrigger, useGSAP }