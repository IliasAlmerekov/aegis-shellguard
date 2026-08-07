import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react'

/* ── Frame sequence ───────────────────────────────────────────────────
   A pre-rendered shot scrubbed by scroll. The caller owns the scroll
   position and pushes progress in; this component owns the canvas, the
   download order and the fit.

   Canvas 2D rather than WebGL on purpose: every frame is a full-viewport
   blit of an already-decoded image, which is exactly what drawImage does.
   A WebGL path would upload 161 textures to hold the same pixels behind an
   extra abstraction and buy nothing back.

   Frames are held as <img> elements and decoded with img.decode(), never as
   ImageBitmap. An ImageBitmap pins its decoded surface for as long as it is
   referenced — 161 × 1920 × 1080 × 4 bytes is over a gigabyte — whereas an
   <img> keeps only the compressed bytes alive and lets the browser evict
   and re-decode the bitmap under pressure. */

const DESKTOP_SET = { dir: '/frames', count: 161 }

/* Every second frame at 960px wide: 81 files, 1.4 MB against the desktop
   set's 7.3 MB. Over the scroll distance a phone actually has, 81 frames
   still land roughly one frame per 15px, so the loss is invisible. */
const MOBILE_SET = { dir: '/frames-sm', count: 81 }

const MAX_IN_FLIGHT = 6
const MAX_DPR = 2

function frameUrl(dir, index) {
  return `${dir}/frame-${String(index + 1).padStart(4, '0')}.webp`
}

function pickSet() {
  if (typeof window === 'undefined') return DESKTOP_SET
  const saveData = navigator.connection?.saveData === true
  const narrow = window.matchMedia('(max-width: 767px)').matches
  return narrow || saveData ? MOBILE_SET : DESKTOP_SET
}

/* Downloads the set in an order that follows the viewer. `cursor` is the
   frame currently on screen, so a viewer who scrubs past the download gets
   the frame they are looking at next rather than waiting out the queue. */
function createLoader({ dir, count }, onFirstFrame) {
  const images = new Array(count)
  const claimed = new Uint8Array(count)
  let inFlight = 0
  let cursor = 0
  let cancelled = false
  let decoded = 0

  function nextIndex() {
    for (let i = cursor; i < count; i++) if (!claimed[i]) return i
    for (let i = 0; i < cursor; i++) if (!claimed[i]) return i
    return -1
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
      // A missing frame is left unclaimed-but-done: the queue moves on and
      // the renderer falls back to the nearest frame it does have.
      img.onerror = () => settle(false)
      img.src = frameUrl(dir, index)
    }
  }

  return {
    count,
    start: pump,
    setCursor(index) {
      cursor = index
      pump()
    },
    /* The sequence is a continuous shot, so the neighbour of a frame that
       has not arrived is very nearly the frame itself — close enough that a
       fast scrub over an incomplete download reads as a lower frame rate
       rather than as a hole. */
    nearest(index) {
      if (images[index]) return images[index]
      for (let d = 1; d < count; d += 1) {
        if (images[index - d]) return images[index - d]
        if (images[index + d]) return images[index + d]
      }
      return null
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
    fit = 'cover',
    focusX = 0.5,
    focusY = 0.5,
    zoom = 1,
    shiftY = 0,
    onReady,
  },
  ref
) {
  const canvasRef = useRef(null)
  const stateRef = useRef({ loader: null, index: 0, fit, focusX, focusY, zoom, shiftY })

  stateRef.current.fit = fit
  stateRef.current.focusX = focusX
  stateRef.current.focusY = focusY
  stateRef.current.zoom = zoom
  stateRef.current.shiftY = shiftY

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return undefined
    const ctx = canvas.getContext('2d', { alpha: false })
    if (!ctx) return undefined
    ctx.imageSmoothingQuality = 'high'

    const state = stateRef.current

    function paint() {
      const img = state.loader?.nearest(state.index)
      const { width: cw, height: ch } = canvas
      if (!cw || !ch) return
      ctx.fillStyle = '#000000'
      ctx.fillRect(0, 0, cw, ch)
      if (!img) return

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
         is free here because the canvas clears to the same black the shot
         itself fades to, so the exposed band is invisible. */
      const shift = Math.max(-0.5, Math.min(0.5, state.shiftY)) * ch
      ctx.drawImage(
        img,
        (cw - dw) * state.focusX,
        (ch - dh) * state.focusY + shift,
        dw,
        dh
      )
    }

    function resize() {
      const dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR)
      const rect = canvas.getBoundingClientRect()
      const w = Math.round(rect.width * dpr)
      const h = Math.round(rect.height * dpr)
      if (!w || !h) return
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w
        canvas.height = h
        ctx.imageSmoothingQuality = 'high'
      }
      paint()
    }

    const loader = createLoader(pickSet(), () => {
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
    // onReady is read through the closure on first paint only; re-running
    // this effect would restart a 7 MB download.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  /* Framing props change at the breakpoint, which does not always resize the
     canvas — repaint explicitly rather than relying on the ResizeObserver. */
  useEffect(() => {
    stateRef.current.paint?.()
  }, [fit, focusX, focusY, zoom, shiftY])

  useImperativeHandle(
    ref,
    () => ({
      /* progress is 0…1 across the whole shot. */
      show(progress) {
        const state = stateRef.current
        const loader = state.loader
        if (!loader) return
        const clamped = progress < 0 ? 0 : progress > 1 ? 1 : progress
        const index = Math.round(clamped * (loader.count - 1))
        if (index === state.index && state.painted) return
        state.index = index
        state.painted = true
        loader.setCursor(index)
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
