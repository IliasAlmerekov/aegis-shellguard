import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react'

/* ── Frame sequence ───────────────────────────────────────────────────
   A pre-rendered shot scrubbed by scroll. The caller owns the scroll
   position and pushes progress in; this component owns the canvas, the
   download order and the fit.

   Canvas 2D rather than WebGL on purpose: every frame is a full-viewport
   blit of an already-decoded image, which is exactly what drawImage does.
   A WebGL path would upload textures to hold the same pixels behind an
   extra abstraction and buy nothing back.

   Frames are held as <img> elements and decoded with img.decode(), never as
   ImageBitmap. An ImageBitmap pins its decoded surface for as long as it is
   referenced — a hundred-odd 1920 × 1080 surfaces is most of a gigabyte —
   whereas an <img> keeps only the compressed bytes alive and lets the browser
   evict and re-decode the bitmap under pressure.

   The unit of rendering is a fractional position, not a frame index. The two
   frames a position falls between are composited at the fractional weight, so
   the shot moves continuously under a continuous scrub instead of stepping
   between the stills it is made of — see `paint`. The same mechanism covers
   frames that have not downloaded yet: the pair widens to whatever has
   arrived and the gap plays as a dissolve. */

/* The render on disk is 161 frames of a machine hand rising out of the dark
   toward a suspended, fractured plate of light: a single directed reach that
   arrives around frame 114 and stops just short of touching.

   Not a loop, and not usable whole. Past the arrival the hand recoils and
   settles, and frames 130–161 are a near-static tail of the same held pose —
   scrubbed, that reads as the gesture undoing itself and then nothing
   happening for a third of the pin. The caller trims it with `range`, which
   is why this component takes a span rather than assuming the whole file
   list; see the Hero. */
const DESKTOP_SET = { dir: '/frames', count: 161 }

/* Every second frame of the same render at 960px wide: 81 files against the
   desktop set's 161, and roughly a fifth of the bytes. Because both sets are
   cut from the same render, a `range` in fractions of the render resolves to
   the same moment in either one — see `resolve`. */
const MOBILE_SET = { dir: '/frames-sm', count: 81 }

const MAX_IN_FLIGHT = 8
const MAX_DPR = 2

/* Ceiling on the backing store, in device pixels, applied by scaling the
   canvas down proportionally so the fit maths is untouched.

   Without it the cost of a frame is set by the visitor's monitor: a 2560px
   window at DPR 2 asks for a 5120 × 2880 surface, and every scrubbed frame
   pays to resample the source into 14.7 megapixels — twice, since frames
   cross-blend. There is nothing to buy with them. The desktop render is
   1920px wide, so anything past roughly that is the resampler inventing
   detail the footage does not contain; capping here costs no visible
   sharpness and keeps the per-frame fill constant across machines, which is
   what makes the scrub hold its rate on a laptop iGPU. */
const MAX_CANVAS_PIXELS = 2_700_000

/* Download passes, coarse to fine. A visitor can reach the end of the shot
   inside a second — long before the whole span has landed. Fetching in scrub
   order would leave that first pass frozen a third of the way in; fetching
   every 8th frame first spends about an eighth of the bytes to make the
   *whole* gesture available immediately, and the later passes fill it in to
   full frame rate. An early scrub reads as a low frame rate version of the
   complete shot rather than as a stall. */
const STRIDES = [8, 4, 2, 1]

function frameUrl(dir, index) {
  return `${dir}/frame-${String(index + 1).padStart(4, '0')}.webp`
}

function pickSet() {
  if (typeof window === 'undefined') return DESKTOP_SET
  const saveData = navigator.connection?.saveData === true
  const narrow = window.matchMedia('(max-width: 767px)').matches
  return narrow || saveData ? MOBILE_SET : DESKTOP_SET
}

/* A fraction of the render resolved to a file index in whichever set is in
   use. Fractions rather than frame numbers because the mobile set samples the
   render by two: frame 113 of 161 is file 57 of 81, and the caller should not
   have to know that. */
function resolve(fraction, count) {
  return Math.round(Math.min(1, Math.max(0, fraction)) * (count - 1))
}

/* Downloads the span coarse to fine, and within each pass in an order that
   follows the viewer. `cursor` is the frame currently on screen, so a viewer
   who scrubs past the download gets the frame they are looking at next rather
   than waiting out the queue. */
function createLoader(set, range, onFirstFrame) {
  const first = resolve(range[0], set.count)
  const last = Math.max(first, resolve(range[1], set.count))
  const count = last - first + 1

  const images = new Array(count)
  const claimed = new Uint8Array(count)
  let inFlight = 0
  let cursor = 0
  let cancelled = false
  let decoded = 0

  function pass(index) {
    for (let i = 0; i < STRIDES.length; i += 1) {
      if (index % STRIDES[i] === 0) return i
    }
    return STRIDES.length - 1
  }

  /* Rank = pass first, then distance from the cursor, counting forward before
     backward. Forward, because the shot plays one way: the frame behind the
     cursor is the one the viewer is least likely to need again. */
  function nextIndex() {
    let best = -1
    let bestRank = Infinity
    for (let i = 0; i < count; i += 1) {
      if (claimed[i]) continue
      const ahead = i >= cursor ? i - cursor : count + (cursor - i)
      const rank = pass(i) * count * 2 + ahead
      if (rank < bestRank) {
        bestRank = rank
        best = i
      }
    }
    return best
  }

  function pump() {
    while (!cancelled && inFlight < MAX_IN_FLIGHT) {
      const index = nextIndex()
      if (index < 0) return
      claimed[index] = 1
      inFlight += 1

      const img = new Image()
      img.decoding = 'async'

      const settle = (loaded) => {
        inFlight -= 1
        if (cancelled) return
        if (loaded) {
          images[index] = img
          decoded += 1
          if (decoded === 1) onFirstFrame()
        }
        pump()
      }

      img.onload = () => {
        // Decode off the main thread before the frame is ever drawn, so
        // scrubbing never pays a decode inside a scroll callback.
        if (img.decode) img.decode().then(() => settle(true), () => settle(true))
        else settle(true)
      }
      // A missing frame is left claimed-but-empty: the queue moves on and the
      // renderer widens its pair around the hole.
      img.onerror = () => settle(false)
      img.src = frameUrl(set.dir, first + index)
    }
  }

  return {
    count,
    start: pump,
    setCursor(index) {
      if (index === cursor) return
      cursor = index
      // Re-ranking the queue is an O(count) scan. It can only change which
      // frame starts next, so there is nothing to gain from running it inside
      // a scroll callback while every slot is already busy.
      if (inFlight < MAX_IN_FLIGHT) pump()
    },
    /* The two frames a fractional position sits between, resolved to whatever
       has actually arrived: back from the floor, forward from the ceiling.
       Once the download completes these are simply the two adjacent frames and
       the renderer cross-fades over a single frame step. While it is still
       coarse they may be eight apart, and the renderer dissolves across the
       gap instead — a soft ghost rather than a hard cut, so an early scrub
       loses temporal detail without ever showing a step.

       Directional, not nearest-first: the pair has to bracket the position, or
       the blend weight would be measured against a frame the shot has already
       passed. */
    back(index) {
      for (let i = Math.min(index, count - 1); i >= 0; i -= 1) {
        if (images[i]) return i
      }
      return -1
    },
    forward(index) {
      for (let i = Math.max(index, 0); i < count; i += 1) {
        if (images[i]) return i
      }
      return -1
    },
    at(index) {
      return images[index] || null
    },
    cancel() {
      cancelled = true
    },
  }
}

export const FrameSequence = forwardRef(function FrameSequence(
  {
    label,
    className,
    /* The span of the render to use, as [from, to] fractions of it. Resolved
       against whichever frame set the device gets, so the same pair names the
       same moment at either breakpoint. */
    range = [0, 1],
    fit = 'cover',
    focusX = 0.5,
    focusY = 0.5,
    zoom = 1,
    shiftY = 0,
    /* What the canvas clears to, and therefore what shows wherever the
       shot does not reach. It has to be the caller's own substrate: the
       clear colour and the page behind the canvas are the same surface as
       far as the eye is concerned. */
    backdrop = '#000000',
    onReady,
  },
  ref
) {
  const canvasRef = useRef(null)
  // `frame` is a *fractional* position in the span, not an index — the
  // renderer blends the two frames it falls between.
  const stateRef = useRef({ loader: null, frame: 0, fit, focusX, focusY, zoom, shiftY, backdrop })

  stateRef.current.fit = fit
  stateRef.current.focusX = focusX
  stateRef.current.focusY = focusY
  stateRef.current.zoom = zoom
  stateRef.current.shiftY = shiftY
  stateRef.current.backdrop = backdrop

  const from = range[0]
  const to = range[1]

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return undefined
    const ctx = canvas.getContext('2d', { alpha: false })
    if (!ctx) return undefined
    ctx.imageSmoothingQuality = 'high'

    const state = stateRef.current

    function blit(img) {
      const { width: cw, height: ch } = canvas
      const iw = img.naturalWidth
      const ih = img.naturalHeight
      const cover = Math.max(cw / iw, ch / ih)
      const scale = (state.fit === 'cover' ? cover : Math.min(cw / iw, ch / ih)) * state.zoom
      const dw = iw * scale
      const dh = ih * scale

      /* `shiftY` moves the shot within the frame, as a fraction of canvas
         height, positive being downward. At the ratios this hero uses,
         cover-fit leaves no vertical slack at all, so `focusY` has nothing to
         move and this is the only real vertical control.

         The shift is deliberately allowed past the image edge. Zooming to
         create slack instead would scale about the centre and push the
         plate — which sits near the top of the source frame — further up,
         the opposite of what a downward nudge is for. Running off the edge
         is free here because the canvas clears to `backdrop`, which the
         caller sets to the same near-black the shot itself fades to, so the
         exposed band is invisible. */
      const shift = Math.max(-0.5, Math.min(0.5, state.shiftY)) * ch
      ctx.drawImage(
        img,
        (cw - dw) * state.focusX,
        (ch - dh) * state.focusY + shift,
        dw,
        dh
      )
    }

    /* Draws the *position*, not a frame.

       The span is a hundred-odd frames over a pin of one or two viewports —
       one frame per ten pixels or so — and a wheel notch is fifty. Snapping to
       the nearest frame therefore shows the shot in jumps of five, however
       smoothly the timeline is being driven underneath: the scrub is
       continuous and the picture is not, and what the eye reports is the
       picture. So the two frames the position falls between are both drawn,
       the second at the fractional weight, and the hand moves continuously
       through the frames instead of between them.

       This is the fix for a slow, deliberate scroll specifically — the case
       where scrub smoothing has no lag to work with and the shot would
       otherwise advance one whole frame at a time.

       Two full-viewport blits per frame is the entire cost, which is why the
       backing store is capped — see MAX_CANVAS_PIXELS. The blend collapses to
       one blit whenever the weight is negligible or only one of the pair has
       downloaded. */
    function paint() {
      const loader = state.loader
      const { width: cw, height: ch } = canvas
      if (!cw || !ch) return
      ctx.fillStyle = state.backdrop
      ctx.fillRect(0, 0, cw, ch)
      if (!loader) return

      const f = state.frame
      const floor = Math.floor(f)
      const lo = loader.back(floor)
      if (lo < 0) return

      const hi = loader.forward(floor + 1)
      let weight = 0
      if (hi > lo) weight = Math.min(1, Math.max(0, (f - lo) / (hi - lo)))

      blit(loader.at(lo))
      // Below a hundredth of a frame step the second blit is invisible and
      // costs as much as the first one.
      if (weight > 0.01) {
        ctx.globalAlpha = weight
        blit(loader.at(hi))
        ctx.globalAlpha = 1
      }
    }

    function resize() {
      const dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR)
      const rect = canvas.getBoundingClientRect()
      let pw = rect.width * dpr
      let ph = rect.height * dpr
      // Scale both axes by the same factor, so the surface keeps the CSS box's
      // aspect and the cover-fit maths above needs to know nothing about this.
      const area = pw * ph
      if (area > MAX_CANVAS_PIXELS) {
        const k = Math.sqrt(MAX_CANVAS_PIXELS / area)
        pw *= k
        ph *= k
      }
      const w = Math.round(pw)
      const h = Math.round(ph)
      if (!w || !h) return
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w
        canvas.height = h
        ctx.imageSmoothingQuality = 'high'
      }
      paint()
    }

    const loader = createLoader(pickSet(), [from, to], () => {
      paint()
      onReady?.()
    })
    state.loader = loader
    state.paint = paint
    loader.start()

    resize()
    const observer = new ResizeObserver(resize)
    observer.observe(canvas)

    return () => {
      observer.disconnect()
      loader.cancel()
      state.loader = null
      state.paint = null
    }
    // onReady is read through the closure on first paint only; re-running this
    // effect would restart the download. The range is a constant of the shot,
    // not a prop that animates.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  /* Framing props change at the breakpoint, which does not always resize the
     canvas — repaint explicitly rather than relying on the ResizeObserver. */
  useEffect(() => {
    stateRef.current.paint?.()
  }, [fit, focusX, focusY, zoom, shiftY, backdrop])

  useImperativeHandle(
    ref,
    () => ({
      /* progress is 0…1 across the requested span, and is deliberately not
         quantised on the way in: the renderer wants the position, and rounding
         it here would throw away exactly the sub-frame detail that keeps the
         scrub continuous.

         Painted synchronously rather than deferred to the next animation
         frame. The caller already drives this from GSAP's ticker, which is a
         rAF callback, so scheduling another one would only put the canvas a
         frame behind the timeline that is supposedly scrubbing it. */
      show(progress) {
        const state = stateRef.current
        const loader = state.loader
        if (!loader) return
        const clamped = progress < 0 ? 0 : progress > 1 ? 1 : progress
        const frame = clamped * (loader.count - 1)
        if (state.painted && frame === state.frame) return
        state.frame = frame
        state.painted = true
        loader.setCursor(Math.round(frame))
        state.paint?.()
      },
      repaint() {
        stateRef.current.paint?.()
      },
    }),
    []
  )

  return (
    <canvas
      ref={canvasRef}
      className={className}
      role="img"
      aria-label={label}
    />
  )
})
