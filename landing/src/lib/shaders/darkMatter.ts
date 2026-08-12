/**
 * The one custom shader in the project.
 *
 * A full-screen quad, a sphere-traced distance field, and everything the
 * matter is made of: the form, the folds, the membrane, the scatter, the
 * veins, the eye and the ground behind them.
 *
 * The numeric constants are interpolated in from `config.ts` at module
 * evaluation rather than passed as uniforms. Two reasons: the compiler can
 * fold them into the march loop, which is the hottest code in the project,
 * and there stays exactly one place where a value lives. The cost is a
 * shader recompile when a value changes, which only happens at author time.
 */

import {
  body,
  eye,
  march,
  material,
  noise,
  palette,
  pointer,
  sss,
  veins,
  warp,
  type Rgb,
} from '../scene/config'

const f = (n: number): string => {
  const s = n.toString()
  return s.includes('.') || s.includes('e') ? s : `${s}.0`
}

const v3 = (v: Rgb | readonly [number, number, number]): string =>
  `vec3(${f(v[0])}, ${f(v[1])}, ${f(v[2])})`

/* The quad is placed directly in clip space, so no matrices are involved and
   the geometry can be a unit plane that never moves. */
export const darkMatterVertex = /* glsl */ `
varying vec2 vUv;

void main() {
  vUv = uv;
  gl_Position = vec4(position.xy * 2.0, 0.0, 1.0);
}
`

const lobeCode = body.lobes
  .map(
    (l) =>
      `  d = smin(d, sphere(p - ${v3(l.position)}, ${f(l.radius)}, ${f(
        l.squash
      )}), ${f(body.smoothness)});`
  )
  .join('\n')

export const darkMatterFragment = /* glsl */ `
precision highp float;

varying vec2 vUv;

uniform float uTime;
uniform vec2  uResolution;

uniform vec3  uCamPos;
uniform vec3  uCamRight;
uniform vec3  uCamUp;
uniform vec3  uCamForward;
uniform float uTanHalfFov;
uniform mat4  uViewProjection;

uniform vec2  uPointer;
uniform float uBreath;
uniform float uSpin;

uniform float uMaxSteps;
uniform float uUseSss;

uniform sampler2D uEnv;
uniform float     uEnvLods;
uniform sampler2D uNormalMap;
uniform sampler2D uRoughnessMap;

#define MAX_STEPS ${march.maxSteps}
#define OCTAVES ${noise.octaves}
#define PI 3.14159265359

/* ── Noise ───────────────────────────────────────────────────────────────
   Value noise on a hashed lattice. Chosen over simplex for cost: this is
   evaluated OCTAVES times per march step per pixel, so the cheapest function
   that still looks organic wins, and at these frequencies the lattice is
   invisible under the domain warp. */

float hash(vec3 p) {
  p = fract(p * 0.3183099 + vec3(0.1, 0.2, 0.3));
  p *= 17.0;
  return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

float valueNoise(vec3 p) {
  vec3 i = floor(p);
  vec3 fr = fract(p);
  // Quintic rather than cubic: the second derivative is continuous too, and
  // a discontinuous one shows up as faint lattice-aligned creases once the
  // field is used as a surface normal.
  vec3 u = fr * fr * fr * (fr * (fr * 6.0 - 15.0) + 10.0);

  return mix(
    mix(mix(hash(i + vec3(0, 0, 0)), hash(i + vec3(1, 0, 0)), u.x),
        mix(hash(i + vec3(0, 1, 0)), hash(i + vec3(1, 1, 0)), u.x), u.y),
    mix(mix(hash(i + vec3(0, 0, 1)), hash(i + vec3(1, 0, 1)), u.x),
        mix(hash(i + vec3(0, 1, 1)), hash(i + vec3(1, 1, 1)), u.x), u.y),
    u.z) * 2.0 - 1.0;
}

float fbm(vec3 p) {
  float sum = 0.0;
  float amp = 0.5;
  for (int i = 0; i < OCTAVES; i++) {
    sum += amp * valueNoise(p);
    p *= 2.02;   // Not exactly 2: an integer ratio lines the octaves' lattices
    amp *= 0.5;  // up and the sum develops a visible grid.
  }
  return sum;
}

/* ── The field ───────────────────────────────────────────────────────────── */

float smin(float a, float b, float k) {
  float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

float sphere(vec3 p, float r, float squash) {
  p.y /= squash;
  return length(p) - r;
}

/* Domain warping: the coordinate space the fractal is sampled in is itself
   pushed around by a broader noise. This is what makes folds flow into each
   other instead of sitting on the surface as bumps. */
vec3 warpSpace(vec3 p) {
  float t = uTime * ${f(warp.timeScale)};
  vec3 q = p * ${f(warp.frequency)} + vec3(0.0, 0.0, t);
  return p + ${f(warp.strength)} * vec3(
    fbm(q),
    fbm(q + vec3(5.2, 1.3, 0.0)),
    fbm(q + vec3(0.0, 9.1, 4.7))
  );
}

float relief(vec3 p) {
  float t = uTime * ${f(noise.timeScale)};
  vec3 w = warpSpace(p);
  return
    fbm(w * ${f(noise.largeFrequency)} + vec3(0.0, t, 0.0)) * ${f(noise.largeAmplitude)} +
    fbm(w * ${f(noise.mediumFrequency)} - vec3(t, 0.0, 0.0)) * ${f(noise.mediumAmplitude)} +
    fbm(w * ${f(noise.smallFrequency)}) * ${f(noise.smallAmplitude)};
}

/** Body only, without the eye. Kept separate so the eye can be shaded from
    its own geometry while still being occluded by the body's folds. */
float bodyField(vec3 p) {
  float d = sphere(p, ${f(body.coreRadius)}, 1.0);
${lobeCode}
  return d - relief(p);
}

float eyeField(vec3 p) {
  return length(p - ${v3(eye.position)}) - ${f(eye.radius)};
}

/** x: distance, y: material id — 0 body, 1 eye. */
vec2 map(vec3 p) {
  float b = bodyField(p);
  float e = eyeField(p);
  // The eye is welded into the same field rather than drawn over it, so the
  // fold in front of it occludes it through the same arithmetic that made
  // the fold. A mesh laid on top always reads as a sticker.
  float d = smin(b, e, ${f(eye.smoothness)});
  return vec2(d, e < b ? 1.0 : 0.0);
}

vec3 calcNormal(vec3 p) {
  vec2 e = vec2(${f(march.normalEpsilon)}, 0.0);
  return normalize(vec3(
    map(p + e.xyy).x - map(p - e.xyy).x,
    map(p + e.yxy).x - map(p - e.yxy).x,
    map(p + e.yyx).x - map(p - e.yyx).x
  ));
}

/* ── Object space ────────────────────────────────────────────────────────
   The mass never rotates in world space; the ray is rotated into its frame
   instead. Same image, and the field stays anchored to its own coordinates
   so the noise does not swim through the surface as the object turns. */

mat3 rotY(float a) {
  float c = cos(a); float s = sin(a);
  return mat3(c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c);
}

mat3 rotX(float a) {
  float c = cos(a); float s = sin(a);
  return mat3(1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c);
}

/* ── Environment ─────────────────────────────────────────────────────────
   The HDR is equirectangular, sampled by direction. Roughness picks a mip
   level, which is a cheap stand-in for a prefiltered radiance map: at these
   roughnesses the difference is a slightly wrong blur on a surface the
   viewer sees for a fraction of a second at a time. */

vec3 sampleEnv(vec3 dir, float roughness) {
  vec2 uv = vec2(
    atan(dir.z, dir.x) / (2.0 * PI) + 0.5,
    acos(clamp(dir.y, -1.0, 1.0)) / PI
  );
  float lod = sqrt(roughness) * uEnvLods;
  return textureLod(uEnv, uv, lod).rgb;
}

/* ── Surface detail ──────────────────────────────────────────────────────
   No UVs exist on a distance field, so the maps are projected on three axes
   and blended by the normal. */

vec3 triplanarNormal(vec3 p, vec3 n) {
  vec3 blend = pow(abs(n), vec3(${f(material.triplanarSharpness)}));
  blend /= dot(blend, vec3(1.0)) + 1e-5;

  float s = ${f(material.triplanarScale)};
  vec3 nx = texture(uNormalMap, p.yz / s).rgb * 2.0 - 1.0;
  vec3 ny = texture(uNormalMap, p.xz / s).rgb * 2.0 - 1.0;
  vec3 nz = texture(uNormalMap, p.xy / s).rgb * 2.0 - 1.0;

  // Whiteout blend: the projected normals are added to the surface normal
  // rather than replacing it, which keeps the low-frequency form intact
  // where two projections meet.
  vec3 bumped =
    blend.x * vec3(nx.z, nx.x, nx.y) +
    blend.y * vec3(ny.x, ny.z, ny.y) +
    blend.z * vec3(nz.x, nz.y, nz.z);

  return normalize(mix(n, normalize(n + bumped), ${f(material.normalStrength)}));
}

float triplanarRoughness(vec3 p, vec3 n) {
  vec3 blend = pow(abs(n), vec3(${f(material.triplanarSharpness)}));
  blend /= dot(blend, vec3(1.0)) + 1e-5;

  float s = ${f(material.triplanarScale)};
  float r =
    blend.x * texture(uRoughnessMap, p.yz / s).r +
    blend.y * texture(uRoughnessMap, p.xz / s).r +
    blend.z * texture(uRoughnessMap, p.xy / s).r;

  return mix(${f(material.roughnessMin)}, ${f(material.roughnessMax)}, r);
}

/* ── Subsurface ──────────────────────────────────────────────────────────
   Thickness by marching back into the field. Where the probe leaves the
   surface early the material is a fin, and a fin lets light through. This is
   the source of nearly all the blue in the scene. */

float thinness(vec3 p, vec3 n) {
  float step = ${f(sss.probeDistance)} / float(${sss.probeSamples});
  float inside = 0.0;
  for (int i = 1; i <= ${sss.probeSamples}; i++) {
    float d = map(p - n * (float(i) * step)).x;
    inside += d < 0.0 ? 1.0 : 0.0;
  }
  // 1 when the probe exited immediately (paper-thin), 0 when it never did.
  return 1.0 - inside / float(${sss.probeSamples});
}

/* ── Veins ───────────────────────────────────────────────────────────────
   The narrow band of a noise field around a threshold, gated by a second
   broad noise so whole regions carry no veins at all. A uniform grid of them
   would read as circuitry rather than as something alive. */

float veinMask(vec3 p) {
  float t = uTime * ${f(veins.timeScale)};
  float n = fbm(p * ${f(veins.frequency)} + vec3(t, -t * 0.6, t * 0.3));
  float band = 1.0 - smoothstep(0.0, ${f(veins.width)}, abs(n - (${f(
    veins.threshold
  )} - 0.5)));

  float region = fbm(p * ${f(veins.maskFrequency)} - vec3(0.0, t * 0.2, 0.0));
  float gate = smoothstep(${f(veins.maskBias)} - 0.5, ${f(veins.maskBias)}, region + 0.5);

  return band * gate;
}

/* ── Eye ─────────────────────────────────────────────────────────────────── */

vec3 shadeEye(vec3 p, vec3 n, vec3 rd) {
  vec3 centre = ${v3(eye.position)};
  vec3 local = normalize(p - centre);

  // The gaze direction: forward, nudged by the pointer. The nudge is small
  // enough that a viewer is never sure it happened.
  vec3 gaze = normalize(vec3(
    uPointer.x * ${f(eye.trackAmount)} * 6.0,
    uPointer.y * ${f(eye.trackAmount)} * 6.0,
    1.0
  ));

  float onAxis = dot(local, gaze);
  float iris = smoothstep(1.0 - ${f(eye.irisScale)} * 0.5, 1.0, onAxis);
  float pupil = smoothstep(1.0 - ${f(eye.pupilScale)} * 0.25, 1.0, onAxis);

  vec3 col = mix(${v3(eye.scleraColor)}, ${v3(eye.irisColor)}, iris);
  col = mix(col, ${v3(eye.pupilColor)}, pupil);

  // A wet highlight, from the environment rather than a fake specular dot, so
  // the eye is lit by the same room as the matter around it.
  vec3 refl = reflect(rd, n);
  col += sampleEnv(refl, 0.05) * 0.35;
  col += ${v3(eye.irisColor)} * iris * (1.0 - pupil) * ${f(eye.irisIntensity)};

  return col;
}

/* ── Body shading ────────────────────────────────────────────────────────── */

vec3 shadeBody(vec3 p, vec3 n, vec3 rd) {
  vec3 nm = triplanarNormal(p, n);
  float rough = triplanarRoughness(p, n);

  vec3 view = -rd;
  float ndv = clamp(dot(nm, view), 0.0, 1.0);

  // Specular from the environment. On a base this dark, this is most of what
  // the viewer actually sees of the surface.
  vec3 refl = reflect(rd, nm);
  vec3 spec = sampleEnv(refl, rough);

  // Schlick, with the dielectric F0 of a wet organic surface.
  float f0 = 0.04;
  float fres = f0 + (1.0 - f0) * pow(1.0 - ndv, 5.0);

  vec3 irradiance = sampleEnv(nm, 1.0);
  vec3 col = ${v3(material.baseColor)} * irradiance * ${f(material.envIntensity)};
  col += spec * fres * ${f(material.envIntensity)} * 4.0;

  // The rim that keeps the silhouette legible against a near-black ground
  // even where no light reaches it.
  float rim = pow(1.0 - ndv, ${f(material.fresnelPower)});
  col += ${v3(material.fresnelColor)} * rim * ${f(material.fresnelStrength)};

  if (uUseSss > 0.5) {
    float thin = pow(thinness(p, nm), ${f(sss.falloff)});
    vec3 scatter = mix(${v3(sss.deepColor)}, ${v3(sss.thinColor)}, thin);
    col += scatter * thin * ${f(sss.strength)};
  }

  float vein = veinMask(p);
  vec3 veinColor = mix(${v3(veins.coreColor)}, ${v3(veins.hotColor)}, vein);
  veinColor = mix(veinColor, ${v3(veins.peakColor)}, smoothstep(0.75, 1.0, vein));
  col += veinColor * vein * ${f(veins.intensity)};

  return col;
}

/* ── Ground ──────────────────────────────────────────────────────────────── */

vec3 background(vec2 uv) {
  // A radial lift toward the centre, barely there. Its job is to stop the
  // frame reading as a flat black rectangle, not to be seen.
  float r = length((uv - 0.5) * vec2(uResolution.x / uResolution.y, 1.0));
  return mix(${v3(palette.nightDeep)}, ${v3(palette.nightVoid)}, smoothstep(0.0, 0.9, r));
}

void main() {
  vec2 uv = vUv;
  vec2 ndc = uv * 2.0 - 1.0;
  float aspect = uResolution.x / uResolution.y;

  vec3 rd = normalize(
    uCamForward +
    uCamRight * ndc.x * uTanHalfFov * aspect +
    uCamUp * ndc.y * uTanHalfFov
  );
  vec3 ro = uCamPos;

  // Into the mass's own frame: the idle drift, then the pointer lean. The
  // mass itself never rotates in world space — rotating the ray keeps the
  // noise anchored to the object's own coordinates, so the folds turn with
  // it instead of swimming through it.
  mat3 frame =
    rotY(-uPointer.x * ${f(pointer.rotateAmount)}) *
    rotX(uPointer.y * ${f(pointer.rotateAmount)}) *
    rotY(-uSpin);

  vec3 roO = frame * ro / uBreath;
  vec3 rdO = normalize(frame * rd);

  float t = 0.0;
  float id = 0.0;
  bool hit = false;

  for (int i = 0; i < MAX_STEPS; i++) {
    if (float(i) >= uMaxSteps) break;

    vec3 p = roO + rdO * t;
    vec2 res = map(p);

    if (res.x < ${f(march.epsilon)}) {
      hit = true;
      id = res.y;
      break;
    }
    if (t > ${f(march.maxDistance)}) break;

    // The noise offset breaks the distance field's Lipschitz bound, so a full
    // step can pass through the surface. Under-stepping restores safety far
    // more cheaply than clamping the noise would.
    t += res.x * ${f(march.stepScale)};
  }

  vec3 col = background(uv);
  float depth = gl_DepthRange.far;

  if (hit) {
    vec3 p = roO + rdO * t;
    vec3 n = calcNormal(p);
    col = id > 0.5 ? shadeEye(p, n, rdO) : shadeBody(p, n, rdO);

    // The quad has no geometry of its own, so the depth the particles test
    // against has to be written by hand — otherwise the cloud the camera
    // flies through would draw entirely in front of the matter or entirely
    // behind it.
    // The frame is a rotation, so its transpose is its inverse — exactly,
    // and without the cost of a general 3x3 inversion per pixel.
    vec3 world = (transpose(frame) * p) * uBreath;
    vec4 clip = uViewProjection * vec4(world, 1.0);
    depth = (clip.z / clip.w) * 0.5 + 0.5;
  }

  gl_FragDepth = clamp(depth, 0.0, 1.0);
  gl_FragColor = vec4(col, 1.0);
}
`
