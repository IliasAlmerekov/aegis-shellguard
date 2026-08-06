# Landing design notes

What is deliberate about the marketing surface, and why. This file covers the
decisions a reader cannot recover from the code alone. Anything mechanical —
the palette, the type scale, the spacing steps — lives as custom properties at
the top of `src/index.css` and is authoritative there; do not restate values
here, they will drift.

## Type

Inter carries prose and headings; JetBrains Mono carries anything the visitor
could paste into a shell. Both are self-hosted variable faces under
`public/fonts` and preloaded from `index.html`, so the swap lands before first
paint instead of reflowing the hero. Weights stay in a low 400–590 band and
tracking stays tight — the page gets its emphasis from scale and contrast, not
from bold.

## Motion

### Hero scroll sequence

**Role:** the one authored moment on the marketing surface.

A 161-frame pre-rendered shot — a machine hand rising toward a fractured plate
of light and stopping short of it — scrubbed by scroll through a Canvas 2D blit
(`src/components/ui/FrameSequence.jsx`), driven by a single GSAP ScrollTrigger
timeline pinned for 150vh.

Frames are held as `<img>` elements decoded ahead of paint, never as
`ImageBitmap`, so the browser can evict decoded surfaces under memory pressure.
The download order follows the viewer's scrub position rather than a fixed
queue, and a frame that has not landed yet falls back to its nearest neighbour
— the sequence degrades in smoothness, never into a blank canvas.

Text accretes over the pin in three beats — headline and actions, then the
subhead, then the install snippet — rather than swapping: nothing already read
is taken away, and the pin releases on the complete composition.

Under `prefers-reduced-motion: reduce` the pin is never created and the hero
renders assembled at the shot's last frame.

Two frame sets ship: `public/frames` at 1920w for `min-width: 768px`, and
`public/frames-sm` at 960w every second frame — 81 files, 1.4 MB against 7.3 MB
— for phones and `save-data`.
