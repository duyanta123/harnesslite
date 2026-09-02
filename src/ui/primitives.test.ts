import { describe, expect, it } from 'vitest'

import { Badge } from '@/ui/Badge'
import { Button } from '@/ui/Button'
import { Modal } from '@/ui/Modal'

/**
 * Phase 0 pilot: the carried primitives arrive verbatim, so this suite only
 * pins the module surface that everything downstream imports — the bodies are
 * owned by v1's accepted tests, and their rendering is verified visually.
 */
describe('carried primitives surface', () => {
  it('exports the three pilot primitives as components', () => {
    expect(typeof Button).toBe('function')
    expect(typeof Badge).toBe('function')
    expect(typeof Modal).toBe('function')
  })
})
