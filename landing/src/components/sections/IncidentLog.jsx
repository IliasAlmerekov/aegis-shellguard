import { useCallback, useEffect, useRef, useState } from 'react'
import { Reveal } from '../ui/Reveal'

// Unedited screenshots of public posts. Nothing here is reconstructed or
// paraphrased — the image is the evidence, and the alt text repeats what the
// post says so the claim survives without the picture.
//
// `href` is the permalink of the post. Leave it empty and the caption renders
// as plain attribution instead of a dead link.
const INCIDENTS = [
  {
    id: 'brunolemos',
    src: '/incidents/in_1.webp',
    width: 750,
    height: 486,
    handle: '@brunolemos',
    href: 'https://x.com/brunolemos/status/2076769881534398974',
    alt:
      'Post by Bruno Lemos (@brunolemos): “GPT-5.6 Sol just deleted my whole production database. ' +
      'That’s it. Not a joke. This had never happened to me before, with any other model, ever. ' +
      'It’s not safe.” The attached agent transcript reads: “Yes. I mistakenly ran destructive ' +
      'integration tests against the Neon database configured in .env. The current production tables ' +
      'are empty. I’m sorry — this should never have happened.”',
  },
  {
    id: 'jasonlk',
    src: '/incidents/in_3.webp',
    width: 750,
    height: 520,
    handle: '@jasonlk',
    href: 'https://x.com/jasonlk/status/1946069562723897802',
    alt:
      'Post by Jason Lemkin (@jasonlk): “.@Replit goes rogue during a code freeze and shutdown and ' +
      'deletes our entire database.” The attached agent reply reads: “Yes. I deleted the entire ' +
      'database without permission during an active code and action freeze.”',
  },
  {
    id: 'simonw',
    src: '/incidents/in_7.webp',
    width: 749,
    height: 428,
    handle: '@simonw',
    href: 'https://x.com/simonw/status/1998447540916936947',
    alt:
      'Post by Simon Willison (@simonw): “Important reminder from Reddit here of the risk you’re ' +
      'taking when you run Claude Code with --dangerously-skip-permissions,” quoting the report: ' +
      '“I found the problem and it’s really bad […] rm -rf tests/ patches/ plan/ ~/ — See that ~/ ' +
      'at the end? That’s your entire home directory.” The attached Reddit post reads: “I was ' +
      'having the Claude CLI clean up my packages in an old repo, and it nuked my whole Mac!”',
  },
  {
    id: 'iamlukethedev',
    src: '/incidents/in_5.webp',
    width: 751,
    height: 419,
    handle: '@iamlukethedev',
    href: 'https://x.com/iamlukethedev/status/2079686197761237430',
    alt:
      'Post by Luke The Dev (@iamlukethedev): “I just watched an AI agent destroy a codebase in 3 ' +
      'seconds.” It lists the commands rm -rf src/, rm -rf tests/ and git reset --hard, then adds: ' +
      '“It ran them all in a single turn. The agent made a logic error early on and cascaded into ' +
      'catastrophic failure.”',
  },
  {
    id: 'intcyberdigest',
    src: '/incidents/in_6.webp',
    width: 751,
    height: 519,
    handle: '@IntCyberDigest',
    href: 'https://x.com/IntCyberDigest/status/2085095341171347658',
    alt:
      'Post by International Cyber Digest (@IntCyberDigest): “Claude Code deleted all of a ' +
      'developer’s user files by mistake and then blamed it on a typo. The developer asked Claude ' +
      'Opus 5 to make a backup. It wrote the backup to the wrong path, then ran a force delete of ' +
      'every user file and folder to clean up its own mistake. The dev says he lost all his files ' +
      'and was left with an agent carrying on like nothing had happened.” The attached terminal ' +
      'capture reads: “I need to stop and check something urgent — that rm -rf was a serious error ' +
      'on my part.”',
  },
  {
    id: 'mattshumer',
    src: '/incidents/in_2.webp',
    width: 750,
    height: 500,
    handle: '@mattshumer_',
    href: 'https://x.com/mattshumer_/status/2075657271401390161',
    alt:
      'Post by Matt Shumer (@mattshumer_): “GPT-5.6-Sol just accidentally deleted almost ALL of my ' +
      'Mac’s files.” The attached agent report reads: “I caused a serious local data-loss ' +
      'incident. A review subagent’s cleanup command expanded $HOME incorrectly and ran: ' +
      'rm -rf /Users/mattsdevbox.”',
  },
  {
    id: 'lifeofjer',
    src: '/incidents/in_4.webp',
    width: 876,
    height: 357,
    handle: '@lifeofjer',
    href: 'https://x.com/lifeofjer/article/2048103471019434248',
    alt:
      'Post by JER (@lifeofjer) headlined “An AI Agent Just Destroyed Our Production Data. ' +
      'It Confessed in Writing.” — 7.2 million views.',
  },
]

function ExternalLinkIcon() {
  return (
    <svg width="11" height="11" viewBox="0 0 14 14" fill="none" aria-hidden="true">
      <path d="M5 3h6v6M11 3 3 11" stroke="currentColor" strokeWidth="1.25" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function Shot({ incident, cloned }) {
  return (
    <li className="incident-shot">
      {/* Nothing here may load lazily: a belt card that has not decoded by the
          time it drifts into view arrives as an empty frame. The clone track
          reuses the same URLs, so it costs nothing beyond a cache hit. */}
      {/* Lazy: the belt drifts at 41 px/s, so a card enters the viewport
          roughly 30 s after the browser's lazy-load rootMargin (~1250 px)
          starts its fetch — far more headroom than a WebP decode needs. The
          clone track reuses the same URLs, so it costs nothing beyond a
          cache hit. Earlier this was `loading="eager"` on the theory that a
          drifting card could arrive undecoded, but 14 × 750 px screenshots
          below the fold is exactly what lazy loading is for. */}
      <img
        src={incident.src}
        width={incident.width}
        height={incident.height}
        alt={cloned ? '' : incident.alt}
        loading="lazy"
        decoding="async"
        draggable="false"
      />
      <p className="incident-shot-caption">
        <span>{incident.handle}</span>
        {incident.href && (
          <a href={incident.href} target="_blank" rel="noreferrer" tabIndex={cloned ? -1 : 0}>
            View post
            <ExternalLinkIcon />
          </a>
        )}
      </p>
    </li>
  )
}

function Belt({ cloned, trackRef }) {
  return (
    <ul className="incident-track" ref={trackRef} aria-hidden={cloned ? 'true' : undefined}>
      {INCIDENTS.map((incident) => (
        <Shot key={incident.id} incident={incident} cloned={cloned} />
      ))}
    </ul>
  )
}

// Drift speed in px/s. Kept in JS rather than CSS because the belt is now
// hand-draggable: a CSS keyframe cannot be picked up mid-flight and handed to a
// pointer, so one rAF loop owns the offset and the drag writes into it.
const DRIFT_PX_PER_SEC = 41
// Per-frame retention of flick velocity at 60fps, and the floor below which the
// glide is done and the steady drift takes back over.
const GLIDE_RETENTION = 0.94
const GLIDE_FLOOR = 0.02
// A press that travels further than this was a drag, not a click on a link.
const DRAG_SLOP = 6

export function IncidentLog() {
  const [dragging, setDragging] = useState(false)
  const [inView, setInView] = useState(false)
  const marqueeRef = useRef(null)
  const beltRef = useRef(null)
  const trackRef = useRef(null)
  const offsetRef = useRef(0)
  const gesture = useRef({
    held: 0,
    dragging: false,
    pointerId: null,
    startX: 0,
    startOffset: 0,
    travelled: 0,
    lastX: 0,
    lastAt: 0,
    velocity: 0,
  })

  const reduceMotion =
    typeof window !== 'undefined' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches

  // The marquee only drifts while it is on screen. The rAF loop below would
  // otherwise run for the whole lifetime of the section — which, since the
  // section is now a lazy chunk that mounts on first render, is the entire
  // page session — writing a transform 60×/s into an element the visitor is
  // not looking at. One observer arms and disarms the loop with a little
  // lead so the drift is already moving as the section enters view.
  useEffect(() => {
    const el = marqueeRef.current
    if (!el || reduceMotion) return undefined
    const io = new IntersectionObserver(
      ([entry]) => setInView(entry.isIntersecting),
      { rootMargin: '100px 0px' }
    )
    io.observe(el)
    return () => io.disconnect()
  }, [reduceMotion])

  useEffect(() => {
    if (reduceMotion || !inView) return undefined

    let frame = 0
    let previous = 0
    let paused = document.hidden

    const tick = (now) => {
      const step = previous ? Math.min(64, now - previous) : 0
      previous = now
      const g = gesture.current
      const span = trackRef.current?.offsetWidth ?? 0

      if (span) {
        if (!g.dragging) {
          if (Math.abs(g.velocity) > GLIDE_FLOOR) {
            offsetRef.current += g.velocity * step
            g.velocity *= GLIDE_RETENTION ** (step / 16.667)
          } else if (!g.held) {
            g.velocity = 0
            offsetRef.current += (DRIFT_PX_PER_SEC * step) / 1000
          }
        }
        // One track's width is the whole cycle: the belt carries two identical
        // tracks, so wrapping here is invisible.
        offsetRef.current = ((offsetRef.current % span) + span) % span
        if (beltRef.current) {
          beltRef.current.style.transform = `translate3d(${-offsetRef.current}px, 0, 0)`
        }
      }

      // The tab being hidden pauses the drift the same way leaving the
      // section does: no point burning a frame nobody sees, and `previous`
      // resets on resume so the belt does not jump.
      if (!paused) frame = requestAnimationFrame(tick)
    }

    const onVisibility = () => {
      const nowHidden = document.hidden
      if (nowHidden && !paused) {
        paused = true
        if (frame) cancelAnimationFrame(frame)
        frame = 0
      } else if (!nowHidden && paused) {
        paused = false
        previous = 0
        frame = requestAnimationFrame(tick)
      }
    }
    document.addEventListener('visibilitychange', onVisibility)

    if (!paused) frame = requestAnimationFrame(tick)
    return () => {
      document.removeEventListener('visibilitychange', onVisibility)
      if (frame) cancelAnimationFrame(frame)
    }
  }, [reduceMotion, inView])

  const hold = useCallback((delta) => {
    gesture.current.held = Math.max(0, gesture.current.held + delta)
  }, [])

  const onPointerDown = (event) => {
    if (reduceMotion) return
    if (event.pointerType === 'mouse' && event.button !== 0) return
    const g = gesture.current
    g.dragging = true
    g.pointerId = event.pointerId
    g.startX = event.clientX
    g.startOffset = offsetRef.current
    g.travelled = 0
    g.lastX = event.clientX
    g.lastAt = event.timeStamp
    g.velocity = 0
    event.currentTarget.setPointerCapture(event.pointerId)
    setDragging(true)
  }

  const onPointerMove = (event) => {
    const g = gesture.current
    if (!g.dragging || event.pointerId !== g.pointerId) return
    const travel = event.clientX - g.startX
    g.travelled = Math.max(g.travelled, Math.abs(travel))
    offsetRef.current = g.startOffset - travel
    const elapsed = event.timeStamp - g.lastAt
    if (elapsed > 0) {
      g.velocity = -(event.clientX - g.lastX) / elapsed
      g.lastX = event.clientX
      g.lastAt = event.timeStamp
    }
  }

  const endDrag = (event) => {
    const g = gesture.current
    if (!g.dragging) return
    g.dragging = false
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    // A pointer that came to rest before release is not a flick — gliding on a
    // velocity sampled 300ms ago would fling the belt out from under the hand.
    if (event.timeStamp - g.lastAt > 120) g.velocity = 0
    g.pointerId = null
    setDragging(false)
  }

  // A drag that passes over a caption link must not open it on release.
  const onClickCapture = (event) => {
    if (gesture.current.travelled > DRAG_SLOP) {
      event.preventDefault()
      event.stopPropagation()
    }
  }

  return (
    <section aria-label="Real incidents where AI agents destroyed work" className="incident-log">
      <div className="mx-auto max-w-[1200px] px-gutter">
        <Reveal className="incident-heading">
          <h2>The “my AI deleted everything” club is getting crowded.</h2>
          <p>
            Seven public posts. Files lost, codebases reset, production databases
            wiped—all by agents with shell access.
          </p>
        </Reveal>
      </div>

      {/* Pointer enter and focus both hold the drift so a reader can finish a
          screenshot; a touch press holds it too, via the drag itself. */}
      <div
        ref={marqueeRef}
        className="incident-marquee"
        data-dragging={dragging ? 'true' : 'false'}
        onPointerEnter={() => hold(1)}
        onPointerLeave={() => hold(-1)}
        onFocus={() => hold(1)}
        onBlur={() => hold(-1)}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onClickCapture={onClickCapture}
      >
        <div className="incident-belt" ref={beltRef}>
          <Belt trackRef={trackRef} />
          <Belt cloned />
        </div>
      </div>
    </section>
  )
}
