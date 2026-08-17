/* Generator for the six isometric figures in the Evidence section.
   Run from the project root:  node scripts/evidence-figures.mjs
   Emits src/components/sections/EvidenceFigures.jsx — edit this file, not
   that one.

   Every figure shares one projection so the six read as one drawing set:
   true 2:1 isometric, +x runs down-right, +y down-left, +z straight up.
   That is the whole reason the drawings are generated rather than drawn:
   six hand-placed wireframes drift apart in projection angle and stroke
   weight, and the drift is exactly what makes a set look bought rather
   than made. Here the projection is one function and the roles are three
   class names, so a seventh figure cannot come out in a different world. */
import { writeFileSync } from 'node:fs'

const C = Math.cos(Math.PI / 6)
const OX = 150
const OY = 138

let S = 34
const P = (x, y, z = 0) => [OX + (x - y) * C * S, OY + ((x + y) * 0.5 - z) * S]
const n = (v) => Math.round(v * 10) / 10
const pts = (list) => list.map((p) => `${n(p[0])},${n(p[1])}`).join(' ')
const poly = (list, attrs = '') => `<polygon points="${pts(list)}"${attrs} />`
const line = (list, attrs = '') => `<polyline points="${pts(list)}"${attrs} />`
const seg = (a, b, attrs = '') => line([a, b], attrs)

/* A box drawn as the three faces isometric actually shows. Returns the top
   face plus the two front faces, so an interior never leaks through a wall. */
function box(x, y, z0, hx, hy, z1, attrs = '') {
  const t = [
    P(x - hx, y - hy, z1),
    P(x + hx, y - hy, z1),
    P(x + hx, y + hy, z1),
    P(x - hx, y + hy, z1),
  ]
  const left = [
    P(x + hx, y - hy, z1),
    P(x + hx, y - hy, z0),
    P(x + hx, y + hy, z0),
    P(x + hx, y + hy, z1),
  ]
  const right = [
    P(x + hx, y + hy, z1),
    P(x + hx, y + hy, z0),
    P(x - hx, y + hy, z0),
    P(x - hx, y + hy, z1),
  ]
  return poly(t, attrs) + poly(left, attrs) + poly(right, attrs)
}

/* A flat rhombus — the top face alone, for plates too thin to show a side. */
const plate = (x, y, z, hx, hy, attrs = '') =>
  poly([P(x - hx, y - hy, z), P(x + hx, y - hy, z), P(x + hx, y + hy, z), P(x - hx, y + hy, z)], attrs)

const DIM = ' class="ev-fig-dim"'
const KEY = ' class="ev-fig-key"'
const DASH = ' class="ev-fig-dash"'

const figures = {}

/* ── FIG 0.1 · Safe path ────────────────────────────────────────────────
   A corridor: floor, two low walls, regular tread marks, and one unbroken
   centreline running the whole length with a marker already well down it.
   The claim is throughput, so the drawing is a route with nothing in it. */
{
  S = 36
  const parts = []
  const X0 = -2.5
  const X1 = 2.5
  const HY = 1.0
  const WZ = 0.85

  parts.push(plate(0, 0, 0, X1, HY, DIM))
  // treads, kept sparse: they measure the run, they are not a texture
  for (let t = X0 + 1; t <= X1 - 0.9; t += 1) {
    parts.push(seg(P(t, -HY, 0), P(t, HY, 0), DIM))
  }
  // walls as open outlines, so the floor keeps reading through them
  parts.push(line([P(X0, -HY, 0), P(X0, -HY, WZ), P(X1, -HY, WZ), P(X1, -HY, 0)], KEY))
  parts.push(line([P(X0, HY, 0), P(X0, HY, WZ), P(X1, HY, WZ), P(X1, HY, 0)], KEY))
  parts.push(seg(P(X0, -HY, WZ), P(X0, HY, WZ), DIM))
  parts.push(seg(P(X1, -HY, WZ), P(X1, HY, WZ), DIM))

  parts.push(seg(P(X0 + 0.2, 0, 0.02), P(X1 - 0.2, 0, 0.02), DASH))
  // The marker is already well down the run, not entering it: the claim is
  // throughput, and a mark sitting at the mouth of the corridor would read
  // as something waiting to be let in.
  parts.push(line([P(0.5, -0.72, 0.04), P(1.75, 0, 0.04), P(0.5, 0.72, 0.04)], KEY))
  figures.safePath = parts.join('')
}

/* ── FIG 0.2 · No telemetry ─────────────────────────────────────────────
   A sealed body with a port cut into its lid and nothing coming out of it:
   three emissions leave the port and each one is struck out before it
   clears the figure. */
{
  S = 34
  const parts = []
  const H = 1.05
  const TOP = 2.0

  parts.push(box(0, 0, 0, H, H, TOP, DIM))
  parts.push(plate(0, 0, TOP + 0.001, 0.34, 0.34, KEY))

  const rays = [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ]
  for (const [dx, dy, dz] of rays) {
    const a = P(dx * 0.45, dy * 0.45, TOP + dz * 0.45)
    const b = P(dx * 1.5, dy * 1.5, TOP + dz * 1.5)
    parts.push(seg(a, b, DASH))
    // struck out at the end of the run
    const k = 6
    parts.push(seg([b[0] - k, b[1] - k], [b[0] + k, b[1] + k], KEY))
    parts.push(seg([b[0] - k, b[1] + k], [b[0] + k, b[1] - k], KEY))
  }
  figures.noTelemetry = parts.join('')
}

/* ── FIG 0.3 · Audit trail ──────────────────────────────────────────────
   One record per decision, each linked to the one before it. The plates
   march away along a single axis and the links between them are the chain
   the claim is actually about. */
{
  S = 33
  const parts = []
  const HX = 0.46
  const HY = 0.7
  const TH = 0.22
  const xs = [-2.55, -1.275, 0, 1.275, 2.55]

  xs.forEach((x, i) => {
    // the link is drawn first so the record it runs into covers its end
    if (i > 0) {
      const px = xs[i - 1] + HX
      const nx = x - HX
      parts.push(seg(P(px, -0.3, TH * 0.5), P(nx, -0.3, TH * 0.5), KEY))
      parts.push(seg(P(px, 0.3, TH * 0.5), P(nx, 0.3, TH * 0.5), KEY))
      const mid = (px + nx) / 2
      parts.push(seg(P(mid, -0.3, TH * 0.5), P(mid, 0.3, TH * 0.5), KEY))
    }
    parts.push(box(x, 0, 0, HX, HY, TH, DIM))
    // the written line on each record — the thing the chain is over
    parts.push(seg(P(x - 0.24, -0.36, TH + 0.002), P(x + 0.24, -0.36, TH + 0.002), KEY))
    parts.push(seg(P(x - 0.24, 0.04, TH + 0.002), P(x + 0.1, 0.04, TH + 0.002), DIM))
    parts.push(seg(P(x - 0.24, 0.4, TH + 0.002), P(x + 0.24, 0.4, TH + 0.002), DIM))
  })
  figures.auditTrail = parts.join('')
}

/* ── FIG 0.4 · Intrinsic blocks ─────────────────────────────────────────
   Seven of them, stacked into a wall, and one trajectory that arrives and
   does not get through. The count is the claim, so the blocks are countable. */
{
  S = 36
  const parts = []
  // Slabs, not cubes: the depth is what decides whether this reads as a wall
  // or as a field of boxes on a floor. Seven of them in a brick bond, because
  // the count is the claim and a reader should be able to count it.
  const HB = 0.26
  const HY = 0.52
  const HT = 0.62
  const row0 = [-1.56, -0.52, 0.52, 1.56]
  const row1 = [-1.04, 0, 1.04]

  for (const y of row0) parts.push(box(0, y, 0, HB, HY, HT, DIM))
  for (const y of row1) parts.push(box(0, y, HT, HB, HY, HT * 2, DIM))

  // the run at the wall, and the deflection off it
  parts.push(seg(P(-2.9, 0, 1.9), P(-0.34, 0, 0.92), DASH))
  parts.push(line([P(-0.34, 0, 0.92), P(-1.3, 0, 2.0), P(-2.5, 0, 2.5)], DASH))
  const tip = P(-2.5, 0, 2.5)
  parts.push(line([[tip[0] + 11, tip[1] - 4], tip, [tip[0] + 4, tip[1] + 10]], KEY))
  // the point of contact
  parts.push(seg(P(-HB, -0.2, 0.92), P(-HB, 0.2, 0.92), KEY))
  figures.intrinsicBlocks = parts.join('')
}

/* ── FIG 0.5 · Config ratchet ───────────────────────────────────────────
   A stair that only climbs, with a pawl seated on it. A cloned config can
   move along this shape in exactly one direction. */
{
  S = 30
  const parts = []
  // Drawn as one extruded profile — treads, risers, and a single silhouette —
  // rather than as a row of boxes. Five boxes side by side put five sets of
  // hidden edges on top of each other and the flight stopped reading as a
  // flight at all.
  //
  // The rise has to beat the tread, and by more than looks right on paper.
  // One step moves the eye down-right by `W·cos30` and up by `R`, so at
  // R = 0.4 against W = 0.62 the flight climbed 3px per 19px of run — a
  // ramp, not a stair. At R = 0.78 the two are equal and it climbs at 45°,
  // which is the angle a staircase is recognised by.
  const HY = 0.9
  const N = 5
  const W = 0.55
  const R = 0.78
  const X0 = -1.375

  for (let i = 0; i < N; i++) {
    const x0 = X0 + i * W
    const z = (i + 1) * R
    // tread
    parts.push(
      poly([P(x0, -HY, z), P(x0 + W, -HY, z), P(x0 + W, HY, z), P(x0, HY, z)], DIM)
    )
    // riser
    parts.push(
      poly([P(x0, -HY, z - R), P(x0, -HY, z), P(x0, HY, z), P(x0, HY, z - R)], DIM)
    )
  }
  // near silhouette, so the flight has an edge instead of a stack of quads
  const sil = [P(X0, HY, 0)]
  for (let i = 0; i < N; i++) {
    const x0 = X0 + i * W
    sil.push(P(x0, HY, (i + 1) * R), P(x0 + W, HY, (i + 1) * R))
  }
  // Closed back along the ground. Left open, the profile ended in a single
  // unanswered 117px drop off the top step — correct geometry for a solid
  // stair, but read as a stray vertical because nothing else in the drawing
  // touched the ground to explain it.
  sil.push(P(X0 + N * W, HY, 0), P(X0, HY, 0))
  parts.push(line(sil, KEY))

  // The pawl, seated in the notch where a riser meets the tread above it —
  // the one place in the drawing that decides which way the thing can move.
  const nx = X0 + 3 * W
  const nz = 4 * R
  parts.push(
    line([P(nx, 0, nz), P(nx - 0.46, 0, nz + 0.5), P(nx - 0.06, 0, nz + 0.62), P(nx, 0, nz)], KEY)
  )

  // one way only: an arrow up the flight, barred at the tail
  const ay = -HY - 0.7
  const t0 = P(X0 + 0.15, ay, 1.15 * R)
  const a = P(X0 + N * W - 0.15, ay, N * R + 0.35)
  parts.push(seg(t0, a, DASH))
  parts.push(line([[a[0] - 11, a[1] - 2], a, [a[0] - 2, a[1] + 11]], KEY))
  parts.push(seg([t0[0] - 6, t0[1] - 6], [t0[0] + 6, t0[1] + 6], KEY))
  figures.configRatchet = parts.join('')
}

/* ── FIG 0.6 · Honest limits ────────────────────────────────────────────
   A rail, not a wall — and the ground it does not cover is drawn, not
   cropped away. The dashed half is the part of the claim most pages leave
   out of the picture. */
{
  S = 34
  const parts = []
  const HY = 1.55
  const EDGE = 0.2
  const FAR = 2.7
  const NEAR = -2.5

  // covered ground, then the ground the rail does not cover — drawn, not
  // cropped. Leaving the far half out of frame would have made the picture
  // claim more than the sentence under it does.
  parts.push(poly([P(NEAR, -HY, 0), P(EDGE, -HY, 0), P(EDGE, HY, 0), P(NEAR, HY, 0)], DIM))
  parts.push(poly([P(EDGE, -HY, 0), P(FAR, -HY, 0), P(FAR, HY, 0), P(EDGE, HY, 0)], DASH))

  const posts = [-HY, -HY / 2, 0, HY / 2, HY]
  for (const y of posts) parts.push(seg(P(EDGE, y, 0), P(EDGE, y, 0.8), KEY))
  parts.push(seg(P(EDGE, -HY, 0.8), P(EDGE, HY, 0.8), KEY))
  parts.push(seg(P(EDGE, -HY, 0.46), P(EDGE, HY, 0.46), KEY))
  figures.honestLimits = parts.join('')
}

const NAMES = {
  safePath: 'FigSafePath',
  noTelemetry: 'FigNoTelemetry',
  auditTrail: 'FigAuditTrail',
  intrinsicBlocks: 'FigIntrinsicBlocks',
  configRatchet: 'FigConfigRatchet',
  honestLimits: 'FigHonestLimits',
}

const header = `/* Generated by .figs.mjs — six isometric figures for the Evidence section.

   One projection for all six (true 2:1 isometric, +x down-right, +y
   down-left, +z up) and one stroke language: \`ev-fig-dim\` is the body of
   the object, \`ev-fig-key\` is the part that carries the claim, \`ev-fig-dash\`
   is the thing that is absent, blocked, or out of scope. Reading the six in
   a row should feel like reading one set of drawings, because it is.

   Fills are none and every stroke inherits \`currentColor\`, so the column's
   hover state moves the whole figure without touching this file. */

`

const body = Object.entries(figures)
  .map(([key, markup]) => {
    const jsx = markup
      .replace(/ class="/g, ' className="')
      .replace(/\/>/g, ' />')
      .replace(/></g, '>\n      <')
    return `export function ${NAMES[key]}() {
  return (
    <svg viewBox="0 0 300 250" fill="none" aria-hidden="true" focusable="false">
      ${jsx}
    </svg>
  )
}`
  })
  .join('\n\n')

writeFileSync('src/components/sections/EvidenceFigures.jsx', header + body + '\n')
console.log('wrote src/components/sections/EvidenceFigures.jsx')
