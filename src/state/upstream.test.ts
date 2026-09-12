import { describe, expect, it } from 'vitest'

import { newerThan } from '@/state/upstream'

describe('newerThan', () => {
  it('orders by the numeric core', () => {
    expect(newerThan('0.1.5-rc.2', '0.1.1-rc.2')).toBe(true)
    expect(newerThan('0.2.0', '0.1.9')).toBe(true)
    expect(newerThan('0.1.5', '0.1.5-rc.2')).toBe(true)
    expect(newerThan('0.1.5-rc.2', '0.1.5')).toBe(false)
    expect(newerThan('0.1.5-rc.2', '0.1.5-rc.2')).toBe(false)
  })

  it('ranks pre-release identifiers numerically where they are numeric', () => {
    expect(newerThan('0.1.5-rc.10', '0.1.5-rc.2')).toBe(true)
    expect(newerThan('0.1.5-rc.2', '0.1.5-rc.10')).toBe(false)
  })

  it('treats unreachable or unparseable answers as no news', () => {
    expect(newerThan(null, '0.1.5-rc.2')).toBe(false)
    expect(newerThan('', '0.1.5-rc.2')).toBe(false)
    expect(newerThan('not a version', '0.1.5-rc.2')).toBe(false)
  })
})
