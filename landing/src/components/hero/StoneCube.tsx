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
  Quaternion,
  RepeatWrapping,
  SRGBColorSpace,
  Texture,
  Vector3,
} from 'three'

import {
  BALANCE,
  CUBE,
  GAP,
  GLOW,
  LIGHT,
  SCROLL,
  SHATTER,
  STONE,
  TIER_SETTINGS,
  type Tier,
} from '../../lib/scene/config'
import { stoneLift, stoneScale } from '../../lib/scene/fit'
import { buildShard, shardGrid } from '../../lib/scene/fracture'
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
    uCrackScale: { value: STONE.crackScale },
    uCrackDarken: { value: STONE.crackDarken },
    uCrackRelief: { value: STONE.crackRelief },
    uCrackRoughen: { value: STONE.crackRoughen },
  }
}

/**
 * Each part gets its own material rather than sharing one, because the core's
 * emissive has to intensify for the part that was lifted — that is, it belongs
 * to that part's state rather than to the cube's. The program is still
 * compiled once: `customProgramCacheKey` is the same for all of them.
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

export function StoneCube({ tier, freeze, progress }: Props) {
  const groupRef = useRef<Group>(null)
  const settings = TIER_SETTINGS[tier]
  const subdivision = settings.subdivision
  const anisotropy = settings.anisotropy

  const geometries = useMemo(
    () =>
      shardGrid().map((shard) => {
        const part = buildShard(shard, subdivision)
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
      shardGrid().map(({ min, max }, index) => {
        const uniforms = createUniforms()
        return {
          uniforms,
          material: createMaterial(uniforms),
          centre: new Vector3().addVectors(new Vector3(...min), new Vector3(...max)).multiplyScalar(0.5),
          // Any fixed spread of axes and shares will do, as long as no two
          // neighbours turn alike: the golden angle never repeats.
          spinAxis: new Vector3().setFromSphericalCoords(1, 1 + index * 2.4, index * 2.4),
          spinShare: 0.4 + 0.6 * ((index * 0.618) % 1),
          flyShare:
            SHATTER.share[0] +
            (SHATTER.share[1] - SHATTER.share[0]) * ((index * 0.382) % 1),
          // Radians per second, and where in its swing the shard starts.
          driftRate:
            (Math.PI * 2) /
            (SHATTER.drift.period[0] +
              (SHATTER.drift.period[1] - SHATTER.drift.period[0]) * ((index * 0.755) % 1)),
          driftPhase: index * 2.4,
        }
      }),
    []
  )

  const invalidate = useThree((state) => state.invalidate)
  const aspect = useThree((state) => state.viewport.aspect)

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

  const modelView = useMemo(() => new Matrix4(), [])
  const partView = useMemo(() => new Matrix4(), [])
  const pivot = useMemo(() => new Vector3(), [])
  const identity = useMemo(() => new Quaternion(), [])
  const map = useMemo(() => densityMap(SCROLL.accent), [])

  useFrame(({ clock, camera }) => {
    const group = groupRef.current
    if (!group) return

    const time = freeze ?? clock.getElapsedTime()
    const stage = stageAt(progress.current ?? 0, SCROLL, map)

    const swing = (axis: number) =>
      BALANCE.pose[axis] +
      Math.sin((time / BALANCE.period[axis]) * Math.PI * 2) * BALANCE.amplitude[axis]

    group.rotation.set(swing(0), swing(1), swing(2))

    group.position.y =
      CUBE.centerY +
      stoneLift(aspect) +
      Math.sin((time / BALANCE.floatPeriod) * Math.PI * 2) * BALANCE.floatAmplitude

    const breath =
      1 + GAP.breathAmplitude * Math.sin((time / GAP.breathPeriod) * Math.PI * 2)

    group.scale.setScalar(stoneScale(aspect))
    group.updateMatrixWorld()
    modelView.multiplyMatrices(camera.matrixWorldInverse, group.matrixWorld)

    const shatter = stage.shatter
    // A phone frame is a third as wide: the same field would fly off it.
    const spreadX = SHATTER.spread[0] * Math.min(1, aspect / SHATTER.spreadAspect)

    for (let index = 0; index < parts.length; index += 1) {
      const part = parts[index]
      const mesh = meshRefs.current[index]
      if (mesh) {
        // A shard turns about its own centre, while the mesh turns about the
        // cube's: the pivot term moves the centre back to where the flight
        // put it.
        const drift = Math.sin(time * part.driftRate + part.driftPhase)
        if (shatter > 0) {
          mesh.quaternion.setFromAxisAngle(
            part.spinAxis,
            (SHATTER.spin * part.spinShare + SHATTER.drift.turn * drift) * shatter
          )
        } else {
          mesh.quaternion.copy(identity)
        }
        pivot.copy(part.centre).applyQuaternion(mesh.quaternion)
        const fly = shatter * part.flyShare
        const bob = SHATTER.drift.bob * drift * shatter
        mesh.position.set(
          part.centre.x * (1 + spreadX * fly) - pivot.x,
          part.centre.y * (1 + SHATTER.spread[1] * fly) - pivot.y + bob,
          part.centre.z * (1 + SHATTER.spread[2] * fly) - pivot.z
        )
        // Each shard turns on its own, so each needs its own normal matrix.
        mesh.updateMatrix()
        partView.multiplyMatrices(modelView, mesh.matrix)
        part.uniforms.uNormalMatrix.value.getNormalMatrix(partView)
      }

      // A shard in flight shows its fracture faces lit; the light settles as
      // the stone closes over them.
      part.uniforms.uGlowGain.value =
        GLOW.restGain * breath + (GLOW.hoverGain - GLOW.restGain * breath) * shatter
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
          // The frame loop writes every shard's matrix itself.
          matrixAutoUpdate={false}
        />
      ))}
    </group>
  )
}
