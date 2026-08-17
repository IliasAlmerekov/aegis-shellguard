// Пережимает набор Rock035 с ambientCG в то, что грузит hero.
//
// Usage: node scripts/rock-to-webp.mjs <rock035-dir> <ground031-dir>
//
// Три файла на выходе, не пять: occlusion, roughness и карта разломов упакованы
// в каналы одной картинки — R = occlusion, G = roughness (конвенция glTF, three
// читает aoMap из .r и roughnessMap из .g), B = разломы. Это треть запросов и
// треть байт против трёх серых картинок, каждая из которых везла бы одно и то же
// значение в трёх каналах.
//
// ## Карта разломов
//
// Трещины на камне не рисуются шумом: шум даёт борозды, а трещина — это ветвящаяся
// сеть с узлами и тупиками, и её статистика шуму не подчиняется. Сеть берётся из
// фотоскана Ground031 (высохшая земля) по его карте смещения: трещина там —
// самая глубокая долина.
//
// Ground031, а не Asphalt013: у асфальта сеть образуют границы щебня, ячейки в
// десяток пикселей, и на камне это читается крапчатостью, а не разломами.
// Размытием это не лечится — топология задана источником, и крупнее она не
// становится. У высохшей земли плиты на порядок крупнее, то есть той же природы,
// что на референсе.
//
// Ищутся трещины не порогом по яркости, а **детектором долин**: маска есть
// разность между размытой картой и самой картой. Тонкий тёмный канал заметно
// темнее своего окружения и даёт большую разность; широкая зернистость темнее
// ровно настолько же, насколько темно её окружение, и разности не даёт.
//
// Простой порог по яркости этого не различает. На шершавом асфальте он вытащил
// ямки между щебнем — и вместо сети получилась крапчатость по всей грани.
//
// Порог поверх разности берётся по процентилю: доля площади под трещинами
// известна заранее, а абсолютные значения зависят от экспозиции скана.
//
// Albedo не затемняется и не обесцвечивается здесь. Тон камня — решение,
// которое придётся крутить, а значит он живёт множителем в config.ts, а не
// запечённым в файл, который потом не отличить от исходника.

import sharp from 'sharp'
import { mkdir } from 'node:fs/promises'
import { join } from 'node:path'

const src = process.argv[2]
const crackSrc = process.argv[3]
if (!src || !crackSrc) {
  console.error('usage: node scripts/rock-to-webp.mjs <rock035-dir> <ground031-dir>')
  process.exit(1)
}

const OUT = 'public/textures/rock'

const SET = 'Rock035_1K-JPG'
const CRACK_SET = 'Ground031_1K-JPG'

// Доля площади под трещинами. Выше — сеть становится кляксами, ниже — рвётся на
// пунктир.
const CRACK_COVERAGE = 0.05
// Радиус размытия для детектора долин, в пикселях при BASE_SIZE. Задаёт, какой
// ширины канал считается трещиной: примерно вдвое меньше радиуса.
const CRACK_BLUR = 15
// Растушёвка порога, в долях диапазона разности. Ноль дал бы ступенчатый край,
// который на близком плане читается лестницей.
const CRACK_SOFTNESS = 0.35

// Normal — единственная карта, работающая на близком плане: в финальном кадре
// трещина занимает весь экран, и микрорельеф там несёт она. Остальным хватает
// 512: у тёмного камня цвет почти однороден, шероховатость тоже.
const NORMAL_SIZE = 1024
const NORMAL_QUALITY = 92

// Замеренная лестница на этом исходнике, если 1024 окажется расточительством:
// 1024@92 653 KB · 1024@88 571 KB · 1024@84 497 KB · 768@92 364 KB · 512@92 160 KB.
// Решается глазами на кадре-ущелье, а не здесь: триплanar тайлит текстуру, так
// что видимая детализация задаётся масштабом наложения не меньше, чем размером.

// Ниже 90 webp начинает класть блоки в наклоны нормали, и это видно не как
// артефакт текстуры, а как гранёная штриховка в затенении.
const BASE_SIZE = 512
const ALBEDO_QUALITY = 82
const ORM_QUALITY = 90

await mkdir(OUT, { recursive: true })

function input(suffix) {
  return join(src, `${SET}_${suffix}.jpg`)
}

async function albedo() {
  const out = join(OUT, 'albedo.webp')
  const info = await sharp(input('Color'))
    .resize(BASE_SIZE, BASE_SIZE, { kernel: 'lanczos3' })
    .webp({ quality: ALBEDO_QUALITY })
    .toFile(out)
  return [out, info.size]
}

// NormalGL, не NormalDX: в WebGL зелёный канал смотрит вверх.
async function normal() {
  const out = join(OUT, 'normal.webp')
  const info = await sharp(input('NormalGL'))
    .resize(NORMAL_SIZE, NORMAL_SIZE, { kernel: 'lanczos3' })
    .webp({ quality: NORMAL_QUALITY })
    .toFile(out)
  return [out, info.size]
}

/**
 * Карта разломов из фотоскана: 255 — внутри трещины, 0 — целая порода.
 *
 * Порог по процентилю, а не по фиксированному значению: он находит трещины при
 * любой экспозиции скана. Растушёвка вокруг порога обязательна — жёсткая граница
 * на близком плане читается лестницей.
 */
function crackMask(depth, blurred) {
  // Долина: насколько точка темнее своего окружения.
  const valley = new Float32Array(depth.length)
  let peak = 0
  for (let i = 0; i < depth.length; i += 1) {
    const d = blurred[i] - depth[i]
    valley[i] = d > 0 ? d : 0
    if (valley[i] > peak) peak = valley[i]
  }
  if (peak === 0) return { mask: Buffer.alloc(depth.length), threshold: 0 }

  // Порог по процентилю: гистограмма по нормированной разности.
  const BINS = 512
  const histogram = new Uint32Array(BINS)
  for (let i = 0; i < depth.length; i += 1) {
    histogram[Math.min(BINS - 1, Math.round((valley[i] / peak) * (BINS - 1)))] += 1
  }

  const target = depth.length * (1 - CRACK_COVERAGE)
  let seen = 0
  let threshold = 1
  for (let bin = 0; bin < BINS; bin += 1) {
    seen += histogram[bin]
    if (seen >= target) {
      threshold = bin / (BINS - 1)
      break
    }
  }

  const softness = Math.max(1 / (BINS - 1), CRACK_SOFTNESS * threshold)
  const mask = Buffer.alloc(depth.length)
  for (let i = 0; i < depth.length; i += 1) {
    const t = (valley[i] / peak - (threshold - softness)) / (2 * softness)
    const clamped = t < 0 ? 0 : t > 1 ? 1 : t
    mask[i] = Math.round(255 * clamped * clamped * (3 - 2 * clamped))
  }

  return { mask, threshold }
}

async function orm() {
  const out = join(OUT, 'orm.webp')

  const plane = (path) =>
    sharp(path)
      .resize(BASE_SIZE, BASE_SIZE, { kernel: 'lanczos3' })
      .greyscale()
      .raw()
      .toBuffer()

  const crackPath = join(crackSrc, `${CRACK_SET}_Displacement.jpg`)

  const [occlusion, roughness, depth, blurred] = await Promise.all([
    plane(input('AmbientOcclusion')),
    plane(input('Roughness')),
    plane(crackPath),
    sharp(crackPath)
      .resize(BASE_SIZE, BASE_SIZE, { kernel: 'lanczos3' })
      .greyscale()
      .blur(CRACK_BLUR)
      .raw()
      .toBuffer(),
  ])

  const { mask, threshold } = crackMask(depth, blurred)
  console.log(
    `crack valley threshold ${threshold.toFixed(3)} at ${CRACK_COVERAGE * 100}% coverage`
  )

  // Маску нужно уметь посмотреть глазами до того, как она попадёт в сцену:
  // крапчатую от сетчатой на глаз в кадре уже не отличить.
  if (process.env.CRACK_PREVIEW) {
    await sharp(mask, { raw: { width: BASE_SIZE, height: BASE_SIZE, channels: 1 } })
      .png()
      .toFile(process.env.CRACK_PREVIEW)
    console.log(`crack preview    ${process.env.CRACK_PREVIEW}`)
  }

  const pixels = BASE_SIZE * BASE_SIZE
  const packed = Buffer.alloc(pixels * 3)
  for (let i = 0; i < pixels; i += 1) {
    packed[i * 3] = occlusion[i]
    packed[i * 3 + 1] = roughness[i]
    packed[i * 3 + 2] = mask[i]
  }

  const info = await sharp(packed, {
    raw: { width: BASE_SIZE, height: BASE_SIZE, channels: 3 },
  })
    .webp({ quality: ORM_QUALITY })
    .toFile(out)
  return [out, info.size]
}

let total = 0
for (const job of [albedo, normal, orm]) {
  const [path, bytes] = await job()
  total += bytes
  console.log(`${path}  ${(bytes / 1024).toFixed(1)} KB`)
}
console.log(`total       ${(total / 1024).toFixed(1)} KB`)
