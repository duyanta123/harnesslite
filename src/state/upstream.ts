/**
 * What the managed Harness upstream is publishing right now.
 *
 * This is the runtime's counterpart of the shell's own signed-update check:
 * one bounded registry look at launch, a quiet failure when the registry is
 * unreachable, and a single piece of information — whether upstream has moved
 * past the release this build was qualified against. Upgrading to an
 * unqualified upstream release is not something the shell can do safely (the
 * qualification patches are written against a known build), so the answer is
 * shown, never acted on automatically.
 */
import { create } from 'zustand'

import { runtimeUpstreamCheck, type RuntimeUpstream } from '@/lib/ipc'

interface UpstreamState {
  /** The last successful check, null until it lands. */
  report: RuntimeUpstream | null
  /** Whether a newer upstream release exists than this build pins. */
  newer: boolean
  check: () => Promise<void>
}

export const useUpstream = create<UpstreamState>((set) => ({
  report: null,
  newer: false,
  check: async () => {
    try {
      const report = await runtimeUpstreamCheck()
      set({ report, newer: newerThan(report.upstream, report.pinned) })
    } catch {
      // The check is a courtesy; a machine that cannot see the registry
      // still runs the release it has.
    }
  },
}))

/**
 * Whether `candidate` is a strictly newer semver release than `baseline`.
 *
 * A pre-release ranks below its own release (`0.1.5-rc.2` < `0.1.5`), and
 * pre-release identifiers compare numerically where they are numeric, so
 * `rc.10` ranks above `rc.2`. Anything unparseable compares as equal — the
 * note is informational and must never show a phantom upgrade.
 */
export function newerThan(candidate: string | null, baseline: string): boolean {
  if (!candidate) return false
  const parse = (version: string): { core: number[]; pre: (number | string)[] } | null => {
    const match = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(version.trim())
    if (!match) return null
    const core = [Number(match[1]), Number(match[2]), Number(match[3])]
    const pre = (match[4] ?? '')
      .split('.')
      .filter((part) => part !== '')
      .map((part) => (/^\d+$/.test(part) ? Number(part) : part))
    return { core, pre }
  }
  const left = parse(candidate)
  const right = parse(baseline)
  if (!left || !right) return false
  for (let index = 0; index < 3; index++) {
    const a = left.core[index] ?? 0
    const b = right.core[index] ?? 0
    if (a !== b) return a > b
  }
  // A release outranks any pre-release of the same core.
  if (left.pre.length === 0 || right.pre.length === 0) {
    return left.pre.length === 0 && right.pre.length > 0
  }
  for (let index = 0; index < Math.max(left.pre.length, right.pre.length); index++) {
    const a = left.pre[index]
    const b = right.pre[index]
    if (a === undefined) return false // fewer identifiers rank lower
    if (b === undefined) return true
    if (a !== b) return typeof a === 'number' && typeof b === 'number' ? a > b : String(a) > String(b)
  }
  return false
}
