import { describe, expect, it } from 'vitest'
import {
  darkMatterFragment,
  darkMatterVertex,
} from '@/lib/shaders/darkMatter'

/**
 * The GLSL lives inside a template literal, which makes it silently fragile in
 * one specific way: a backtick anywhere in a comment ends the string early,
 * and the shader ships truncated. TypeScript sometimes catches the fallout as
 * a syntax error further down the file and sometimes does not, so the source
 * is asserted directly.
 */
describe('dark matter shader source', () => {
  it('is not truncated', () => {
    expect(darkMatterVertex).toContain('void main()')
    expect(darkMatterFragment).toContain('void main()')
    // The last thing the fragment shader does.
    expect(darkMatterFragment).toContain('fragColor =')
  })

  it('declares its own fragment output, which GLSL3 requires', () => {
    expect(darkMatterFragment).toMatch(/out\s+vec4\s+fragColor/)
    // Assignment, not the word: the comment above the declaration names
    // gl_FragColor to explain why it is absent.
    expect(darkMatterFragment).not.toMatch(/gl_FragColor\s*=/)
  })

  it('interpolates config values rather than leaving placeholders', () => {
    expect(darkMatterFragment).not.toContain('${')
    expect(darkMatterFragment).not.toContain('NaN')
    expect(darkMatterFragment).not.toContain('undefined')
  })

  it('emits every float as a float', () => {
    // `vec3(0, 0, 0)` is legal GLSL, but a bare `0` where a float is expected
    // is not — and the interpolation helper is the only thing standing
    // between a whole-number config value and that error.
    const bareIntArgs = darkMatterFragment.match(/smin\([^)]*,\s*\d+\s*\)/g)
    expect(bareIntArgs).toBeNull()
  })

  it('declares every uniform the renderer sets', () => {
    for (const name of [
      'uTime',
      'uResolution',
      'uCamPos',
      'uCamRight',
      'uCamUp',
      'uCamForward',
      'uTanHalfFov',
      'uViewProjection',
      'uPointer',
      'uBreath',
      'uSpin',
      'uMaxSteps',
      'uUseSss',
      'uEnv',
      'uEnvLods',
      'uNormalMap',
      'uRoughnessMap',
    ]) {
      expect(darkMatterFragment).toContain(name)
    }
  })
})
