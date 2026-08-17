/**
 * The triplanar stone, injected into `MeshStandardMaterial`.
 *
 * (Both Standard and Physical compile from the same `meshphysical` source, so
 * every injection point below is shared by the two; `StoneCube.tsx` explains
 * why the material is the cheaper of the pair.)
 *
 * ## Why triplanar and not UV
 *
 * After the vertices are displaced, a box's UVs are stretched unevenly, and
 * the fracture surfaces have nowhere to get UVs from at all: they came from a
 * cut, not from an unwrap. Three axis-aligned projections settle both
 * questions at once and require no UV attribute — which the geometry does not
 * carry anyway.
 *
 * ## Why the maps are bound as our own uniforms rather than material slots
 *
 * Handing the textures to `map` / `normalMap` / `aoMap` would make three
 * declare UV varyings for them and demand the matching attributes, which the
 * geometry does not have. Our own uniforms give exactly what is needed and
 * nothing spare in the shader.
 *
 * ## About `uNormalMatrix`
 *
 * The blended normal comes out in object space, while lighting needs it in
 * view space. `normalMatrix` is available in the vertex shader but not in the
 * fragment shader, so it is passed as a uniform of our own.
 *
 * One uniform for all four parts is legitimate because the parts only
 * **translate** — both the resting gap and the lift under the cursor are
 * translations, and a translation does not affect the normal matrix. Only the
 * group as a whole rotates, identically for all four. If a part ever starts
 * rotating on its own this stops being true — and it would also break the
 * proof that neighbours cannot intersect, so both places are guarded by the
 * same prohibition.
 */

export const STONE_VERTEX_HEAD = /* glsl */ `
attribute vec3 aRest;
attribute float aCut;

varying vec3 vObjPos;
varying vec3 vObjNormal;
varying vec3 vRest;
varying float vCut;
`

export const STONE_VERTEX_NORMAL = /* glsl */ `
vObjNormal = objectNormal;
`

export const STONE_VERTEX_BODY = /* glsl */ `
vObjPos = transformed;
vRest = aRest;
vCut = aCut;
`

export const STONE_FRAGMENT_HEAD = /* glsl */ `
varying vec3 vObjPos;
varying vec3 vObjNormal;
varying vec3 vRest;
varying float vCut;

uniform sampler2D uAlbedo;
uniform sampler2D uNormal;
uniform sampler2D uOrm;

uniform mat3 uNormalMatrix;

uniform vec3 uTint;
uniform float uTriplanarScale;
uniform vec3 uSkew;
uniform float uNormalStrength;
uniform float uRoughness;
uniform float uAoIntensity;
uniform float uDetailScale;
uniform float uDetailSkew;
uniform float uDetailMix;

uniform float uHalf;
uniform vec3 uGlowCore;
uniform vec3 uGlowBody;
uniform float uGlowIntensity;
uniform float uMouthLevel;
uniform float uConcentration;
uniform float uGlowGain;
uniform float uLockPhase;
uniform float uLockStrength;
uniform float uCrackScale;
uniform float uCrackDarken;
uniform float uCrackRelief;
uniform float uCrackRoughen;
uniform float uCrackVeinReach;
uniform float uCrackVeinIntensity;
uniform float uSpillWidth;
uniform float uSpillIntensity;
uniform float uSpillFalloff;

vec2 rotateUv(vec2 uv, float turns) {
  float a = turns * 6.2831853;
  float s = sin(a);
  float c = cos(a);
  return vec2(uv.x * c - uv.y * s, uv.x * s + uv.y * c);
}

// Projection weights. A power of 4 is sharp enough that almost one projection
// works on a cube face, and soft enough that on a chipped edge the transition
// does not read as a seam.
vec3 triplanarBlend(vec3 n) {
  vec3 blend = pow(abs(n), vec3(4.0));
  return blend / max(blend.x + blend.y + blend.z, 1e-5);
}

/**
 * The threshold below which a projection is not taken.
 *
 * The weights are normalised, so 0.004 is four thousandths of a contribution
 * to the colour: less than the low bit of an eight-bit channel. At a power of
 * 4 such a weight is reached already 14° off axis, which means that over
 * almost the whole area of every cube face **one** projection out of three is
 * doing the work, and the other two only on chipped edges and corners.
 *
 * Skipping is legitimate precisely because the branch here is coherent: a face
 * goes down one branch as a whole, and wavefronts diverge only over a narrow
 * band along an edge. A branch that diverged at every pixel would cost more
 * than the fetch it saves.
 */
#define TRI_EPS 0.004

/**
 * A projection with its derivatives already computed.
 *
 * The derivatives must be computed **before** the branch and stored. An
 * ordinary fetch under non-uniform control flow takes its mip level from
 * derivatives that are undefined at that point, and the result is garbage
 * along face boundaries. A fetch with an explicit gradient receives them as a
 * parameter, so skipping a branch spoils nothing.
 *
 * The derivatives are computed once per projection set and reused by every
 * map: albedo, orm and the normal all walk the same coordinates.
 */
struct Proj {
  vec2 uv;
  vec2 dx;
  vec2 dy;
  float weight;
};

struct Projections {
  Proj x;
  Proj y;
  Proj z;
};

Proj makeProj(vec2 uv, float weight) {
  Proj p;
  p.uv = uv;
  p.dx = dFdx(uv);
  p.dy = dFdy(uv);
  p.weight = weight;
  return p;
}

// The three projections get different rotations: otherwise the stone's own
// layering would coincide with itself where projections meet and announce
// every boundary.
Projections projections(vec3 p, vec3 blend) {
  Projections uv;
  uv.x = makeProj(rotateUv(p.zy * uTriplanarScale, uSkew.x), blend.x);
  uv.y = makeProj(rotateUv(p.xz * uTriplanarScale, uSkew.y), blend.y);
  uv.z = makeProj(rotateUv(p.xy * uTriplanarScale, uSkew.z), blend.z);
  return uv;
}

vec4 triplanarSample(sampler2D tex, Projections uv) {
  vec4 sum = vec4(0.0);
  if (uv.x.weight > TRI_EPS) sum += textureGrad(tex, uv.x.uv, uv.x.dx, uv.x.dy) * uv.x.weight;
  if (uv.y.weight > TRI_EPS) sum += textureGrad(tex, uv.y.uv, uv.y.dx, uv.y.dy) * uv.y.weight;
  if (uv.z.weight > TRI_EPS) sum += textureGrad(tex, uv.z.uv, uv.z.dx, uv.z.dy) * uv.z.weight;
  return sum;
}

/**
 * Projections for the crack network.
 *
 * A scale of its own rather than one shared with the rock, because these are
 * different physical quantities: the stone's grain is millimetres, the
 * fracture network is centimetres. At a shared scale the network came out
 * speckled — its cells became finer than the cavities.
 */
Projections crackProjections(vec3 p, vec3 blend) {
  Projections uv;
  float s = uTriplanarScale * uCrackScale;
  uv.x = makeProj(rotateUv(p.zy * s, uSkew.x), blend.x);
  uv.y = makeProj(rotateUv(p.xz * s, uSkew.y), blend.y);
  uv.z = makeProj(rotateUv(p.xy * s, uSkew.z), blend.z);
  return uv;
}

/**
 * The second overlay octave — against tiling.
 *
 * Rock035 is layered, and one octave repeated across a face folds its layering
 * onto itself: a regular woven pattern appears on the face, and that is what
 * gives computer graphics away. A second fetch at a different scale and under
 * a different rotation breaks the period — the patterns can only coincide
 * where both scales coincide, and they are incommensurable.
 *
 * The scale multiplier is deliberately not round: at 2 or 3 the octaves would
 * fall back onto one lattice.
 */
Projections detailProjections(vec3 p, vec3 blend) {
  Projections uv;
  float s = uTriplanarScale * uDetailScale;
  uv.x = makeProj(rotateUv(p.zy * s, uSkew.x + uDetailSkew), blend.x);
  uv.y = makeProj(rotateUv(p.xz * s, uSkew.y - uDetailSkew), blend.y);
  uv.z = makeProj(rotateUv(p.xy * s, uSkew.z + uDetailSkew * 1.7), blend.z);
  return uv;
}

/**
 * Normal blending by the "whiteout" method: the slopes are added to the
 * geometric normal rather than carried by a TBN matrix, which triplanar does
 * not have. Plainly averaging the three fetches instead would flatten the
 * relief on edges, where the weights are comparable.
 */
vec3 triplanarNormal(Projections uv, vec3 n) {
  vec3 sum = vec3(0.0);

  if (uv.x.weight > TRI_EPS) {
    vec3 nx = textureGrad(uNormal, uv.x.uv, uv.x.dx, uv.x.dy).xyz * 2.0 - 1.0;
    nx.xy *= uNormalStrength;
    sum += vec3(nx.xy + n.zy, abs(nx.z) * n.x).zyx * uv.x.weight;
  }
  if (uv.y.weight > TRI_EPS) {
    vec3 ny = textureGrad(uNormal, uv.y.uv, uv.y.dx, uv.y.dy).xyz * 2.0 - 1.0;
    ny.xy *= uNormalStrength;
    sum += vec3(ny.xy + n.xz, abs(ny.z) * n.y).xzy * uv.y.weight;
  }
  if (uv.z.weight > TRI_EPS) {
    vec3 nz = textureGrad(uNormal, uv.z.uv, uv.z.dx, uv.z.dy).xyz * 2.0 - 1.0;
    nz.xy *= uNormalStrength;
    sum += vec3(nz.xy + n.xy, abs(nz.z) * n.z) * uv.z.weight;
  }

  return normalize(sum);
}
`

/**
 * One fetch per map, computed once: albedo, orm and the normal share the same
 * weights and the same projections.
 */
export const STONE_FRAGMENT_MAPS = /* glsl */ `
vec3 objNormal = normalize(vObjNormal);
vec3 blend = triplanarBlend(objNormal);
Projections uv = projections(vObjPos, blend);

Projections detail = detailProjections(vObjPos, blend);

vec4 albedo = mix(
  triplanarSample(uAlbedo, uv),
  triplanarSample(uAlbedo, detail),
  uDetailMix
);
vec3 ormBase = triplanarSample(uOrm, uv).rgb;
vec3 ormDetail = triplanarSample(uOrm, detail).rgb;
vec3 orm = mix(ormBase, ormDetail, uDetailMix);

// The crack network — the blue channel, but from its own projection: it has a
// physical scale of its own, coarser than the rock's grain. One octave, not
// two: the network has to read as a network with junctions and dead ends, and
// laying two networks over each other turns it into speckle.
float crack = triplanarSample(uOrm, crackProjections(vObjPos, blend)).b;

// A crack is a dark, rough cavity, not a line of paint.
diffuseColor.rgb *= albedo.rgb * uTint * (1.0 - uCrackDarken * crack);
`

export const STONE_FRAGMENT_NORMAL = /* glsl */ `
// The normals of the two octaves are added, not blended: slopes are additive
// quantities, and averaging would flatten both. The second octave is taken at
// a lower weight — it breaks the period, it does not set the relief.
vec3 baseNormal = triplanarNormal(uv, objNormal);
vec3 detailNormal = triplanarNormal(detail, objNormal);
vec3 mixedNormal = normalize(baseNormal + detailNormal * uDetailMix);

normal = normalize(uNormalMatrix * mixedNormal);

// Crack relief from the derivatives of the mask: where the mask changes
// sharply, the rim of a cavity passes, and the normal has to tip into it. This
// costs not a single extra fetch.
//
// The tilt is applied **after** the move into view space, not before: screen
// derivatives are aligned with the screen, and in view space the X and Y axes
// are aligned with the screen too. In object space these would be two
// unrelated frames, and the tilt would wander off in a random direction as the
// stone rotates.
vec2 slope = vec2(dFdx(crack), dFdy(crack));
normal = normalize(normal - vec3(slope * uCrackRelief, 0.0));
`

/**
 * The core.
 *
 * Depth inside the cube is taken as the max-norm of the vertex position
 * **before displacement**: on the outer surface it equals the half-extent, at
 * the centre it is zero. So `inner = 1 - d/half` is an honest measure of how
 * hidden a point is inside the stone, and emissive rising with it reads as a
 * source in the depths.
 *
 * At the mouth of the fissure `uMouthLevel` remains — at rest that is the
 * entire visible light. When a part moves away, a deeper band of surface is
 * exposed to the camera and the glow intensifies on its own, with no separate
 * brightness animation.
 *
 * The unevenness is not decoration: an even glow reads as a lamp under a slot,
 * a mottled one as heated rock with individual facets of the break alight.
 */
export const STONE_FRAGMENT_EMISSIVE = /* glsl */ `
float glow = 0.0;

if (vCut > 0.5) {
  vec3 a = abs(vRest);
  float depth = max(max(a.x, a.y), a.z) / uHalf;
  float inner = clamp(1.0 - depth, 0.0, 1.0);

  float core = pow(inner, uConcentration);
  glow = mix(uMouthLevel, 1.0, core);
} else {
  // The spill across the stone. This is what is visible in the reference: not
  // a slot, but a lit lip of the break and a soft falloff across the slab. The
  // falloff exponent presses the light against the lip itself — a linear
  // falloff would give a broad washed-out wash.
  float toSeam = min(abs(vRest.x), abs(vRest.y)) / uHalf;
  float lip = 1.0 - smoothstep(0.0, uSpillWidth, toSeam);
  glow = uSpillIntensity * pow(lip, uSpillFalloff);

  // The spill's unevenness comes from the crack network below, not from a
  // random lattice.
  //
  // There used to be a hash lattice here: a random value per cell with no
  // filtering at all. Such a lattice aliases at any distance where a cell
  // becomes smaller than a pixel: sparkle in stills, shimmer in motion, and no
  // mipmap saves it, because it is computed rather than sampled from a
  // texture. The crack network comes from a texture and is mipmap-filtered.

  // Veins. Light leaves the fracture **along the rock's real cracks**, fading
  // with distance. This is what separates a photograph from a drawn outline:
  // in a photograph the main fracture is not alone, a network with junctions
  // and dead ends runs off it, glowing more faintly the further from the
  // source.
  float reach = 1.0 - smoothstep(0.0, uCrackVeinReach, toSeam);
  glow += uCrackVeinIntensity * crack * reach * reach;
}

// Policy Lock — one wave from the centre to the edges of the fracture. Its
// position depends only on scroll progress, so scrolling back rewinds the
// pulse honestly.
float lockDistance = length(vRest.xy) / (uHalf * 1.41421356);
float lockBand = 1.0 - smoothstep(0.0, 0.085, abs(lockDistance - uLockPhase));
// The wave lights not only the already-glowing lip but the crack network on
// the rock as well. Otherwise multiplying a near-zero emissive gave near zero
// and the pulse vanished.
float lockVein = mix(0.075, 0.42, crack);
glow += lockBand * lockVein * uLockStrength;

// The colour is biased toward blue: cyan is reserved for the hottest places. A
// linear mix carried the whole fissure into turquoise, whereas in the
// reference turquoise is the highlight on a lip, not the tone of the entire
// fracture.
float heat = clamp(glow, 0.0, 1.0);
vec3 glowColor = mix(uGlowBody, uGlowCore, heat * heat);
totalEmissiveRadiance = glowColor * glow * uGlowIntensity * uGlowGain;
`

export const STONE_FRAGMENT_ROUGHNESS = /* glsl */ `
float roughnessFactor = clamp(orm.g * uRoughness + uCrackRoughen * crack, 0.04, 1.0);
`

export const STONE_FRAGMENT_AO = /* glsl */ `
float ambientOcclusion = 1.0 - uAoIntensity * (1.0 - orm.r);
reflectedLight.indirectDiffuse *= ambientOcclusion;
#if defined( USE_CLEARCOAT )
  clearcoatSpecularIndirect *= ambientOcclusion;
#endif
#if defined( USE_SHEEN )
  sheenSpecularIndirect *= ambientOcclusion;
#endif
#if defined( USE_ENVMAP ) && defined( STANDARD )
  float dotNV = saturate( dot( geometryNormal, geometryViewDir ) );
  reflectedLight.indirectSpecular *= computeSpecularOcclusion( dotNV, ambientOcclusion, material.roughness );
#endif
`
