import { describe, expect, it } from 'vitest'

import { freezeFromSearch } from '../lib/scene/quality'

describe('freezeFromSearch', () => {
  it('ignores an empty freeze value', () => {
    expect(freezeFromSearch('?freeze=')).toBeNull()
  })
})
