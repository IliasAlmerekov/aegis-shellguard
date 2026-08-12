# Third-party assets

Everything the hero scene loads at runtime is CC0 — public domain, no
attribution required. It is recorded anyway, because a licence that is not
written down is a licence nobody can check later.

## `membrane/normal.webp`, `membrane/roughness.webp`

- Source: [Leather037](https://ambientcg.com/view?id=Leather037) — ambientCG
- Licence: CC0 1.0
- Original: 1K JPG, `NormalGL` and `Roughness` channels

Resized to 512×512 and re-encoded as WebP. The maps are micro-relief on a
near-black membrane seen from across the hero, tiled triplanar — 1K carried
detail no viewer can resolve at 2.4 MB, and 512 carries the same read at
under 200 KB for the pair.

`NormalGL`, not `NormalDX`: WebGL's green channel points up.

The name in the archive says leather, and the scene is not leather. The map
is used for one job — an even, non-directional grain that breaks the perfect
smoothness of a procedural surface. Woven fabric was rejected for this: its
weave is directional, and a directional pattern tiled through three
triplanar projections announces every projection boundary.

## `../env/studio-1k.hdr`

- Source: [Ferndale Studio 01](https://polyhaven.com/a/ferndale_studio_01) — Poly Haven
- Author: Greg Zaal
- Licence: CC0 1.0
- Original: 1K HDR, unmodified

A photo studio with a dark ceiling and one dominant softbox. Chosen for what
it does *not* contain: no landscape, no windows, no coloured practicals. On a
near-black membrane the environment's only visible job is the specular streak
it lays along a fold, and a busy environment turns that streak into legible
clutter.

Neutral white on purpose. Every blue in the scene comes from subsurface
scatter and the electric veins, which are authored — an environment that
tinted the reflections too would make that budget impossible to control.

1K rather than 2K or 4K: the surface is rough enough that its reflections are
blurred past the point where added environment resolution survives, so the
larger files buy nothing a visitor can see.
