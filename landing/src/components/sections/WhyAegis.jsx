import { useRef } from 'react'
import gsap from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'
import { useGSAP } from '@gsap/react'

gsap.registerPlugin(useGSAP, ScrollTrigger)

/* The two beats of the section, split so the tone can change mid-sentence:
   `dim` is body-weight steel, `bright` is the haze the hero reserves for the
   words that carry the claim. The palette is the hero's — nothing new is
   introduced between the two surfaces. */
const LINE_ONE = [
  { text: 'One unchecked AI command can', tone: 'dim' },
  { text: 'delete files, reset commits, or drop tables.', tone: 'bright' },
]

const LINE_TWO = [
  { text: 'Aegis pauses risky commands for approval', tone: 'bright' },
  { text: 'and blocks catastrophic ones.', tone: 'dim' },
]

/* Each word is its own inline-block so the entrance can stagger across the
   line instead of sliding the whole paragraph as one slab. The space is a
   real text node between spans — putting it inside the span would stop the
   line from wrapping. */
function Words({ segments }) {
  const out = []
  let key = 0
  for (const seg of segments) {
    for (const word of seg.text.split(' ')) {
      out.push(
        <span
          key={key++}
          data-word
          className={`inline-block will-change-transform ${
            seg.tone === 'bright' ? 'text-haze' : 'text-steel'
          }`}
          style={{ fontWeight: seg.tone === 'bright' ? 520 : 400 }}
        >
          {word}
        </span>
      )
      out.push(' ')
    }
  }
  return out
}

export function WhyAegis() {
  const rootRef = useRef(null)
  const copyRef = useRef(null)

  useGSAP(
    () => {
      const mm = gsap.matchMedia()

      /* Reduced motion gets the finished composition: both sentences in
         place, the rule already drawn. Nothing moves. */
      mm.add('(prefers-reduced-motion: reduce)', () => {
        gsap.set('[data-word]', { opacity: 1, y: 0, filter: 'none' })
        gsap.set('[data-rule-fill]', { scaleX: 1 })
      })

      mm.add(
        {
          motion: '(prefers-reduced-motion: no-preference)',
          isDesktop: '(min-width: 768px)',
        },
        (ctx) => {
          if (!ctx.conditions.motion) return
          const { isDesktop } = ctx.conditions

          /* The travel is vertical. Sending the words in from off-page left
             and right made the whole block swing sideways under the reader —
             the line arrives now by settling onto its own baseline, which
             holds the centred column dead still. Short enough that the word
             is legible before it lands. */
          const rise = isDesktop ? 22 : 14

          /* Hidden here rather than in CSS so the copy still renders if the
             bundle never arrives. */
          gsap.set('[data-line="one"] [data-word]', { y: rise, opacity: 0, filter: 'blur(12px)' })
          gsap.set('[data-line="two"] [data-word]', { y: rise, opacity: 0, filter: 'blur(12px)' })
          gsap.set('[data-rule-fill]', { scaleX: 0, transformOrigin: 'left center' })

          /* Scroll only decides *when*; the timeline owns its own tempo, so
             the sentences read at a speed the visitor cannot scrub, stall, or
             run backwards. The first sentence lands before the second starts:
             the order of the argument is the order of the motion. */
          const tl = gsap.timeline({ paused: true, defaults: { ease: 'expo.out' } })

          tl.to('[data-line="one"] [data-word]', {
            y: 0,
            opacity: 1,
            filter: 'blur(0px)',
            duration: 1.25,
            stagger: { each: 0.045, from: 'start' },
          })
            .to('[data-rule-fill]', { scaleX: 1, duration: 1.1, ease: 'power3.inOut' }, '<0.45')
            .to(
              '[data-line="two"] [data-word]',
              {
                y: 0,
                opacity: 1,
                filter: 'blur(0px)',
                duration: 1.25,
                stagger: { each: 0.045, from: 'end' },
              },
              '<0.15'
            )

          /* The run repeats when the visitor comes back to the block, but only
             after they actually left it. Scroll position alone cannot express
             that — every boundary tight enough to catch a return is also a
             boundary the visitor crosses while reading, which is what made
             half-scrolling back inside the block start the words over.

             So the replay is armed rather than positional: leaving the block
             entirely arms it, running it spends the arming. Reversing direction
             mid-sentence crosses boundaries but changes nothing, because there
             is nothing armed to spend. */
          let armed = true

          const run = () => {
            if (!armed) return
            armed = false
            tl.restart()
          }

          /* Parking is the one thing that arms a replay, and it snaps the words
             back off-page with no tween — so it may only happen while nothing is
             on screen. These boundaries are the block's full exit in either
             direction, which is also exactly the gesture the visitor means by
             having scrolled past the text. */
          const park = ScrollTrigger.create({
            trigger: copyRef.current,
            start: 'top bottom',
            end: 'bottom top',
            onLeave: () => {
              armed = true
              tl.pause(0)
            },
            onLeaveBack: () => {
              armed = true
              tl.pause(0)
            },
          })

          /* Playing wants the block in frame: anchored to the copy rather than
             the section, since the section is a full viewport tall and centres
             its type — measured from its top, the words would arrive while
             still below the fold. Arriving from above is the mirror case, so the
             window closes late enough that the whole block is on screen before
             the run starts. */
          const runIn = ScrollTrigger.create({
            trigger: copyRef.current,
            start: 'top 78%',
            end: 'bottom 55%',
            onEnter: run,
            onEnterBack: run,
          })

          /* A visitor whose browser restores a scroll position inside the
             window never crosses either boundary, so the state at mount has to
             be read rather than waited for. */
          if (runIn.isActive) run()

          return () => {
            runIn.kill()
            park.kill()
            tl.kill()
          }
        }
      )

      /* Where the block starts is measured in the layout, and the layout
         moves if the face swaps in late. */
      document.fonts?.ready.then(() => ScrollTrigger.refresh())

      return () => mm.revert()
    },
    { scope: rootRef }
  )

  return (
    /* No background of its own: the hero's shot ends on pure black and the
       page canvas is the same black, so the two surfaces are one field and
       there is no seam to see. `overflow-hidden` contains the words while
       they are still off-page. */
    <section
      id="why-aegis"
      ref={rootRef}
      aria-label="Why Aegis"
      className="relative flex min-h-svh items-center overflow-hidden py-28 md:py-40"
    >
      <div ref={copyRef} className="mx-auto w-full max-w-[1200px] px-6">
        <div data-line="one" className="max-w-[880px] will-change-transform">
          <p className="font-inter-variable text-[26px] leading-[1.18] tracking-[-0.6px] sm:text-heading-sm sm:leading-heading-sm sm:tracking-heading-sm md:text-heading md:leading-[1.08] md:tracking-heading">
            <Words segments={LINE_ONE} />
          </p>
        </div>

        {/* A hairline that draws left-to-right between the two claims — the
            hinge the second sentence swings back from. */}
        <div aria-hidden="true" className="my-8 h-px w-full bg-iron md:my-12">
          <div data-rule-fill className="h-full w-full bg-tidal" />
        </div>

        <div data-line="two" className="ml-auto max-w-[880px] text-right will-change-transform">
          <p className="font-inter-variable text-[26px] leading-[1.18] tracking-[-0.6px] sm:text-heading-sm sm:leading-heading-sm sm:tracking-heading-sm md:text-heading md:leading-[1.08] md:tracking-heading">
            <Words segments={LINE_TWO} />
          </p>
        </div>
      </div>
    </section>
  )
}
