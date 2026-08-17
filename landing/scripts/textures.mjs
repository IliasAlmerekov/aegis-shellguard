#!/usr/bin/env node
/**
 * Производные карты для сцены героя.
 *
 * Исходники в `public/textures/rock` — авторские, правятся руками и лежат в
 * репозитории как есть. Всё, что из них выводится, собирается здесь, а не
 * подкладывается разово: вариант, собранный один раз в чьей-то консоли,
 * расходится с исходником на первой же правке и расхождение находится через
 * месяц на чужом телефоне.
 *
 * Запуск: `node scripts/textures.mjs`
 */

import { mkdir, stat, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

import sharp from 'sharp'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const DIR = path.join(ROOT, 'public/textures/rock')

/**
 * Телефонный вариант карты нормалей.
 *
 * Она одна весит 653 КБ — больше, чем albedo и orm вместе взятые в четыре
 * раза, — потому что единственная из трёх идёт в 1024². Половина стороны и
 * q92 дают 158 КБ: на телефоне карта ложится на куб, который занимает треть
 * тех экранных пикселей, что на десктопе, и разницы в нём не видно.
 *
 * Потерь боятся не зря — на карте нормалей артефакт компрессии читается как
 * складка камня, которой нет. Поэтому 92, а не «как получится»: ниже начинает
 * зернить подсвеченная кромка разлома, ради которой сцена и существует.
 */
const VARIANTS = [
  {
    from: 'normal.webp',
    to: 'normal-512.webp',
    size: 512,
    quality: 92,
  },
]

const kb = (bytes) => `${Math.round(bytes / 1024)} КБ`

await mkdir(DIR, { recursive: true })

for (const { from, to, size, quality } of VARIANTS) {
  const source = path.join(DIR, from)
  const target = path.join(DIR, to)

  const buffer = await sharp(source)
    .resize(size, size, { kernel: 'lanczos3' })
    .webp({ quality, effort: 6 })
    .toBuffer()

  await writeFile(target, buffer)

  const before = await stat(source)
  console.log(`${from} ${kb(before.size)} → ${to} ${kb(buffer.length)}`)
}
