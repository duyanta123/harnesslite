import { describe, expect, it } from 'vitest'

import {
  EVENT_CHANNEL,
  LINK_CHANNEL,
  NODE_CHANNEL,
  REMOTE_CHANNEL,
  SHARED_CHANNEL,
  TERMINAL_EXIT,
  TERMINAL_OUTPUT,
} from '@/lib/ipc'

/**
 * The event channels are a contract shared with `hd-core/src/contract.rs`,
 * where a test pins them to the `harnesslite://` scheme. These are the exact
 * same strings: a rename on either side that is not mirrored on the other
 * silences the UI with no error anywhere, which is what the first release
 * shipped with — start worked, the shell just never heard about it.
 */
describe('event channels match contract.rs', () => {
  it('supervisor events', () => expect(EVENT_CHANNEL).toBe('harnesslite://harness'))
  it('node install progress', () => expect(NODE_CHANNEL).toBe('harnesslite://node/progress'))
  it('remote door', () => expect(REMOTE_CHANNEL).toBe('harnesslite://remote'))
  it('terminal output', () => expect(TERMINAL_OUTPUT).toBe('harnesslite://terminal/output'))
  it('terminal exit', () => expect(TERMINAL_EXIT).toBe('harnesslite://terminal/exit'))
  it('cross-window announcements', () => expect(SHARED_CHANNEL).toBe('harnesslite://announce'))
  it('deep links', () => expect(LINK_CHANNEL).toBe('harnesslite://desktop/link'))
})
