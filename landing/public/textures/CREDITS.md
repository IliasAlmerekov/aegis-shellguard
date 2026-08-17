# Third-party assets

Everything the hero scene loads at runtime is CC0 — public domain, no
attribution required. It is recorded anyway, because a licence that is not
written down is a licence nobody can check later.

## `rock/albedo.webp`, `rock/normal.webp`, `rock/orm.webp`

Two CC0 scans, three files.

- Stone: [Rock035](https://ambientcg.com/view?id=Rock035) — ambientCG, CC0 1.0.
  1K JPG, `Color`, `NormalGL`, `Roughness` and `AmbientOcclusion` channels.
- Crack network: [Ground031](https://ambientcg.com/view?id=Ground031) —
  ambientCG, CC0 1.0. 1K JPG, `Displacement` channel only.

Re-encoded by `scripts/rock-to-webp.mjs`; the phone-sized normal map is derived
from the result by `scripts/textures.mjs`. Albedo 512, normal 1024, and one 512
image carrying three unrelated channels: R = occlusion, G = roughness,
B = crack network. R and G follow the glTF convention, which three.js reads
directly — `aoMap` takes `.r` and `roughnessMap` takes `.g`, so one texture
serves both slots — and B rides along for free.

### Why the cracks come from a scan and not from noise

Noise draws grooves. A crack network has junctions, branches and dead ends, and
that statistic is what the eye recognises as real; no amount of tuned noise
produces it. The network is extracted from Ground031's displacement by a valley
detector — the difference between the blurred map and the map itself, which a
thin dark channel produces and broad grain does not.

Ground031 rather than the obvious candidate, Asphalt013: asphalt's network is
the boundaries between pieces of aggregate, cells about ten pixels across, and
on stone that reads as speckle rather than as fractures. Blurring does not fix
it — the topology belongs to the source. Dried earth has plates an order of
magnitude larger, which is the same nature as the reference photograph.

The network is sampled at its own triplanar scale, coarser than the stone's:
grain is millimetres and fractures are centimetres, and on a shared scale the
network's cells fall below the stone's own pitting and stop reading as cracks.

`NormalGL`, not `NormalDX`: WebGL's green channel points up.

Rock035 rather than any of the lighter scans in the same library, because the
cube hangs in near-black. A light albedo would have to be crushed to fit the
scene, and crushing a light texture amplifies its noise — the result is grime,
not stone. This one is already dark, and its own cast is blue-black, so the
electric blue coming out of the fissures lands on the stone as a continuation
rather than as a foreign colour on grey.

Its relief is blocky — large facets with chipped edges — rather than layered.
That matters twice: it is what a fractured cube should look like, and a
directional pattern tiled through three triplanar projections announces every
projection boundary. Rock035 is not perfectly non-directional (no photoscanned
rock is), so the three projections are given different rotation and scale, and
their boundaries do not coincide with the stone's own grain.

Albedo is not darkened or desaturated in the re-encode. The stone's tone is a
number that has to be tuned, so it lives as a multiplier in
`src/lib/scene/config.ts` — not baked into a file that could no longer be told
apart from the source.

## `../hdri/studio-small-08-512.hdr`

[Studio Small 08](https://polyhaven.com/a/studio_small_08) by Sergej Majboroda,
from Poly Haven under CC0. The 1K Radiance HDR source was reduced to 512×256
with ffmpeg. It is used only as the PBR environment map; the photographed room
is never rendered as the scene background.
