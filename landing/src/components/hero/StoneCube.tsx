'use client'

import { useEffect, useMemo, useRef, type RefObject } from 'react'
import { useFrame, useThree } from '@react-three/fiber'
import { useTexture } from '@react-three/drei'
import {
  BufferAttribute,
  BufferGeometry,
  Color,
  Group,
  Mesh,
  LinearSRGBColorSpace,
  Matrix3,
  Matrix4,
  MeshStandardMaterial,
  RepeatWrapping,
  SRGBColorSpace,
  Texture,
} from 'three'

import {
  BALANCE,
  CUBE,
  GAP,
  GLOW,
  HOVER,
  LIGHT,
  SCROLL,
  STONE,
  TIER_SETTINGS,
  type Tier,
} from '../../lib/scene/config'
import { QUADRANTS, buildPart, partDirection } from '../../lib/scene/fracture'
import { approach, tauFor } from '../../lib/scene/hover'
import { densityMap, stageAt } from '../../lib/scene/progress'
import {
  STONE_FRAGMENT_AO,
  STONE_FRAGMENT_EMISSIVE,
  STONE_FRAGMENT_HEAD,
  STONE_FRAGMENT_MAPS,
  STONE_FRAGMENT_NORMAL,
  STONE_FRAGMENT_ROUGHNESS,
  STONE_VERTEX_BODY,
  STONE_VERTEX_HEAD,
  STONE_VERTEX_NORMAL,
} from '../../lib/shaders/stone'

type Props = {
  tier: Tier
  /**
   * Whether the stone answers the cursor. False on touch devices and under
   * `prefers-reduced-motion`.
   */
  interactive: boolean
  /**
   * Pin progress, 0..1. A ref rather than a value: scroll arrives per frame,
   * and React state would re-render the whole scene every frame for the sake
   * of a single number.
   */
  progress: RefObject<number>
  /**
   * Frozen scene time in seconds, in place of the scene's own clock. The
   * capture scripts need it: two frames taken with different numbers in
   * `config.ts` are comparable only if the stone stands identically in both.
   */
  freeze?: number
}

/** Opting out of raycasting, by three's convention: the ray simply finds nothing. */
const NO_RAYCAST = () => {}

/**
 * The normal map, sized to the screen. Built by `scripts/textures.mjs`.
 *
 * It alone is 1024² and weighs 653 KB — four times the albedo and the orm map
 * together. A phone gets 512² for 158 KB: the cube occupies a third of the
 * screen pixels there that it does on desktop, so the difference is invisible
 * in it, while half a megabyte over a mobile network is extremely visible.
 *
 * The decision is made **once, when the module loads**, not from state and not
 * from the quality tier. `useTexture` suspends the component by URL as the
 * key: changing the URL after the first render means a second request and a
 * second fall back into the fallback. The tier, moreover, starts at `full` and
 * is refined by an effect, so a phone would have pulled both maps. The module
 * is loaded dynamically and on the client only (`ssr: false` in `Hero.tsx`),
 * so `window` exists here; rotating the screen does not change the map — there
 * is no reason to change it, it is already in video memory.
 *
 * The query is the same one used in `layout.tsx`: were they to diverge, one
 * file would be preloaded and a different one fetched — precisely the double
 * download all of this was written to remove.
 */
const NORMAL_MAP =
  typeof window !== 'undefined' &&
  window.matchMedia('(max-width: 767.98px)').matches
    ? '/textures/rock/normal-512.webp'
    : '/textures/rock/normal.webp'

type PartUniforms = ReturnType<typeof createUniforms>

function createUniforms() {
  return {
    uAlbedo: { value: null as Texture | null },
    uNormal: { value: null as Texture | null },
    uOrm: { value: null as Texture | null },
    uNormalMatrix: { value: new Matrix3() },
    uTint: { value: new Color(STONE.tint) },
    uTriplanarScale: { value: STONE.triplanarScale },
    uSkew: { value: [...STONE.projectionSkew] },
    uDetailScale: { value: STONE.detailScale },
    uDetailSkew: { value: STONE.detailSkew },
    uDetailMix: { value: STONE.detailMix },
    uNormalStrength: { value: STONE.normalStrength },
    uRoughness: { value: STONE.roughness },
    uAoIntensity: { value: STONE.aoIntensity },
    uHalf: { value: CUBE.size / 2 },
    uGlowCore: { value: new Color(GLOW.core) },
    uGlowBody: { value: new Color(GLOW.body) },
    uGlowIntensity: { value: GLOW.intensity },
    uMouthLevel: { value: GLOW.mouthLevel },
    uConcentration: { value: GLOW.concentration },
    // Changes every frame — the type has to be wider than the config literal.
    uGlowGain: { value: GLOW.restGain as number },
    uLockPhase: { value: 0 },
    uLockStrength: { value: 0 },
    uCrackScale: { value: STONE.crackScale },
    uCrackDarken: { value: STONE.crackDarken },
    uCrackRelief: { value: STONE.crackRelief },
    uCrackRoughen: { value: STONE.crackRoughen },
    uCrackVeinReach: { value: GLOW.veinReach },
    uCrackVeinIntensity: { value: GLOW.veinIntensity },
    uSpillWidth: { value: GLOW.spillWidth },
    uSpillIntensity: { value: GLOW.spillIntensity },
    uSpillFalloff: { value: GLOW.spillFalloff },
  }
}

/**
 * Each part gets its own material rather than sharing one, because the core's
 * emissive has to intensify for the part that was lifted — that is, it belongs
 * to that part's state rather than to the cube's. The program is still
 * compiled once: `customProgramCacheKey` is the same for all four.
 *
 * The material is `MeshStandardMaterial`, not `MeshPhysicalMaterial`.
 *
 * Physical differs from Standard by a set of add-ons: clearcoat, sheen,
 * transmission, iridescence, anisotropic reflection. The stone uses none of
 * them. What Physical does bring unconditionally is its own specular response
 * model through `ior` and `specularColor` — at default values it yields
 * exactly the same F0 = 0.04 as Standard, but computed in every fragment. The
 * swap changes not one pixel and removes that arithmetic entirely.
 *
 * Both materials compile from the same `meshphysical` source, so every
 * injection point below stays where it was.
 */
function createMaterial(uniforms: PartUniforms) {
  const material = new MeshStandardMaterial({
    color: 0xffffff,
    roughness: 1,
    metalness: 0,
    // The stone does not shine with lacquer, but with no reflections at all
    // PBR looks like matte fill. The reflections come from the environment,
    // not from the light sources.
    envMapIntensity: LIGHT.envIntensity,
    // Emissive is overwritten wholesale by the shader; white is only here so
    // that three declares the uniform.
    emissive: 0xffffff,
  })

  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, uniforms)

    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', `#include <common>\n${STONE_VERTEX_HEAD}`)
      .replace(
        '#include <beginnormal_vertex>',
        `#include <beginnormal_vertex>\n${STONE_VERTEX_NORMAL}`
      )
      .replace('#include <begin_vertex>', `#include <begin_vertex>\n${STONE_VERTEX_BODY}`)

    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', `#include <common>\n${STONE_FRAGMENT_HEAD}`)
      .replace('#include <map_fragment>', STONE_FRAGMENT_MAPS)
      .replace('#include <normal_fragment_maps>', STONE_FRAGMENT_NORMAL)
      .replace('#include <roughnessmap_fragment>', STONE_FRAGMENT_ROUGHNESS)
      .replace('#include <emissivemap_fragment>', STONE_FRAGMENT_EMISSIVE)
      .replace('#include <aomap_fragment>', STONE_FRAGMENT_AO)
  }

  material.customProgramCacheKey = () => 'aegis-stone'

  return material
}

export function StoneCube({ tier, freeze, interactive, progress }: Props) {
  const groupRef = useRef<Group>(null)
  const settings = TIER_SETTINGS[tier]
  const subdivision = settings.subdivision
  const anisotropy = settings.anisotropy

  const geometries = useMemo(
    () =>
      QUADRANTS.map((quadrant) => {
        const part = buildPart(quadrant, subdivision)
        const geometry = new BufferGeometry()
        geometry.setAttribute('position', new BufferAttribute(part.position, 3))
        geometry.setAttribute('normal', new BufferAttribute(part.normal, 3))
        geometry.setAttribute('aRest', new BufferAttribute(part.rest, 3))
        geometry.setAttribute('aCut', new BufferAttribute(part.cut, 1))
        geometry.setIndex(new BufferAttribute(part.index, 1))
        geometry.computeBoundingSphere()
        return geometry
      }),
    [subdivision]
  )

  const parts = useMemo(
    () =>
      QUADRANTS.map((quadrant) => {
        const uniforms = createUniforms()
        const [dx, dy, dz] = partDirection(quadrant, 0)
        // The lift direction is constant: `upBias` is a config constant.
        // Computing it per frame would mean building the same array four times
        // a frame for the same result.
        const [lx, ly, lz] = partDirection(quadrant, HOVER.upBias)
        return {
          uniforms,
          material: createMaterial(uniforms),
          rest: [dx * GAP.rest, dy * GAP.rest, dz * GAP.rest] as [
            number,
            number,
            number,
          ],
          lift: [lx, ly, lz] as [number, number, number],
        }
      }),
    []
  )

  const invalidate = useThree((state) => state.invalidate)

  const [albedo, normal, orm] = useTexture([
    '/textures/rock/albedo.webp',
    NORMAL_MAP,
    '/textures/rock/orm.webp',
  ])

  useEffect(() => {
    for (const [texture, isColor] of [
      [albedo, true],
      [normal, false],
      [orm, false],
    ] as const) {
      texture.wrapS = RepeatWrapping
      texture.wrapT = RepeatWrapping
      texture.colorSpace = isColor ? SRGBColorSpace : LinearSRGBColorSpace
      texture.anisotropy = anisotropy
      texture.needsUpdate = true
    }

    for (const part of parts) {
      part.uniforms.uAlbedo.value = albedo
      part.uniforms.uNormal.value = normal
      part.uniforms.uOrm.value = orm
    }

    // The clock may be stopped: under `prefers-reduced-motion` the canvas
    // renders on demand, and without an explicit nudge the maps would arrive
    // for a frame that is never going to happen.
    invalidate()
  }, [albedo, normal, orm, parts, anisotropy, invalidate])

  useEffect(() => {
    const owned = geometries
    return () => {
      for (const geometry of owned) geometry.dispose()
    }
  }, [geometries])

  useEffect(() => {
    const owned = parts
    return () => {
      for (const part of owned) part.material.dispose()
    }
  }, [parts])

  const meshRefs = useRef<(Mesh | null)[]>([])

  /**
   * The opening amount — one value for the whole stone, because all four parts
   * move apart at once regardless of where the pointer landed.
   *
   * A `ref`, not state: the gesture runs per frame, and a React re-render on
   * every frame of motion is both unnecessary and harmful — it would churn the
   * entire scene subtree for a number only `useFrame` reads.
   */
  const lift = useRef(0)
  const hovered = useRef(false)

  const modelView = useMemo(() => new Matrix4(), [])
  // The normal matrix is the same for all four parts — the parts only
  // translate. The inverse-transpose is computed once per frame rather than
  // four times, and there is nothing left for it to drift against.
  const normalMatrix = useMemo(() => new Matrix3(), [])
  const map = useMemo(() => densityMap(SCROLL.accent), [])

  useFrame(({ clock, camera }, delta) => {
    const group = groupRef.current
    if (!group) return

    const time = freeze ?? clock.getElapsedTime()
    const stage = stageAt(progress.current ?? 0, SCROLL, map)

    // The balancing fades out as the camera closes in, while the pose turns to
    // face front. Two systems rotating one object at once produce jitter where
    // they meet; here the second enters exactly as much as the first leaves.
    const idle = 1 - stage.approach
    const wobble = (axis: number) =>
      Math.sin((time / BALANCE.period[axis]) * Math.PI * 2) * BALANCE.amplitude[axis] * idle

    const toFace = (axis: number) => {
      const rest = BALANCE.pose[axis] + wobble(axis)
      return rest + (SCROLL.facePose[axis] - rest) * stage.approach
    }

    group.rotation.set(toFace(0), toFace(1), toFace(2))

    group.position.y =
      CUBE.centerY +
      Math.sin((time / BALANCE.floatPeriod) * Math.PI * 2) * BALANCE.floatAmplitude * idle

    const breath =
      1 + GAP.breathAmplitude * Math.sin((time / GAP.breathPeriod) * Math.PI * 2)

    // One normal matrix for all four parts: the parts only translate, and
    // translation does not affect it. Only the group rotates, identically for
    // all of them.
    group.updateMatrixWorld()
    modelView.multiplyMatrices(camera.matrixWorldInverse, group.matrixWorld)

    // The cursor response is released as soon as the scroll starts: the parts'
    // displacement must have exactly one author per frame. Otherwise the
    // scene's main gesture lands on a pose that is already taken, and "the
    // stone came apart" reads more weakly — it has partly happened already.
    const restingStill = (progress.current ?? 0) < 0.02
    const target = interactive && restingStill && hovered.current ? 1 : 0
    const hoverAmount = approach(
      lift.current,
      target,
      delta,
      tauFor(lift.current, target)
    )
    lift.current = hoverAmount

    // Scroll and cursor move the parts along the same direction, so their
    // contributions add without risk of intersection: the proof that
    // neighbours cannot meet is about direction, not magnitude.
    const spread =
      HOVER.lift * hoverAmount +
      SCROLL.open * stage.opening -
      SCROLL.lockRecoil * stage.lock
    const amount = spread / HOVER.lift

    // Halfway through the lock the whole stone compresses for one beat, then
    // returns to its scale. That makes the moment of contact legible even
    // where the light pulse is lost on a bright display.
    const lockImpact = Math.sin(Math.PI * stage.lock)
    const impactScale = 1 - SCROLL.lockImpactScale * lockImpact
    group.scale.setScalar(impactScale)

    normalMatrix.getNormalMatrix(modelView)

    for (let index = 0; index < parts.length; index += 1) {
      const part = parts[index]
      part.uniforms.uNormalMatrix.value.copy(normalMatrix)
      part.uniforms.uLockPhase.value = stage.pulse
      part.uniforms.uLockStrength.value =
        Math.sin(Math.PI * stage.pulse) * GLOW.lockPulse

      const mesh = meshRefs.current[index]
      if (mesh) {
        mesh.position.set(
          part.rest[0] + part.lift[0] * spread,
          part.rest[1] + part.lift[1] * spread,
          part.rest[2] + part.lift[2] * spread
        )
      }

      // The brightness is not animated separately: it is a consequence of a
      // departing part exposing a deeper — and therefore brighter — band of
      // fracture surface to the camera. The multiplier only brings the effect
      // up to the required strength, which is why motion and flash cannot fall
      // out of step.
      const opened = Math.min(1, amount)
      part.uniforms.uGlowGain.value =
        GLOW.restGain * breath + (GLOW.hoverGain - GLOW.restGain * breath) * opened
    }
  })

  return (
    <group ref={groupRef} name="stone-cube">
      {geometries.map((geometry, index) => (
        <mesh
          key={index}
          ref={(mesh) => {
            meshRefs.current[index] = mesh
          }}
          geometry={geometry}
          material={parts[index].material}
          position={parts[index].rest}
          // The stone is taken out of the ray's path: hits are computed
          // against the four simple boxes below. Otherwise every mouse move
          // would run a ray across two hundred thousand triangles.
          raycast={NO_RAYCAST}
        />
      ))}

      {/*
        The cursor trap. One box for the whole cube rather than four per
        quarter: all the parts open at once, and there is no need to know which
        one the pointer hit — it is enough that it is on the stone.

        A plain box instead of the displaced mesh: the error at the boundary is
        smaller than a jag of the fracture, and it costs twelve triangles
        instead of two hundred thousand.

        Not `visible={false}`: three excludes invisible objects from
        raycasting, and the trap would stop trapping. So the object is visible
        but writes nothing — neither colour nor depth.
      */}
      <mesh
        onPointerOver={() => {
          hovered.current = true
        }}
        onPointerMove={() => {
          hovered.current = true
        }}
        onPointerOut={() => {
          hovered.current = false
        }}
      >
        <boxGeometry args={[CUBE.size, CUBE.size, CUBE.size]} />
        <meshBasicMaterial colorWrite={false} depthWrite={false} transparent opacity={0} />
      </mesh>

    </group>
  )
}
