#!/usr/bin/env node
/**
 * Release asset gate: the bundle step must have produced the installer and the
 * portable executable, and the installer must name the tag being released.
 * Cheap checks that keep an empty release from shipping.
 */
import { existsSync, readdirSync, statSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const nsis = join(root, 'src-tauri', 'target', 'release', 'bundle', 'nsis')
const expectedTag = process.env.EXPECTED_TAG ?? ''

const failures = []

if (!existsSync(nsis)) failures.push('no nsis bundle directory — the bundle step did not run')

const installers = existsSync(nsis)
  ? readdirSync(nsis).filter((name) => name.endsWith('-setup.exe'))
  : []
if (!installers.length) failures.push('no *-setup.exe installer in the nsis bundle directory')
// Tauri names installers after the version, so the tag's `v` prefix is not
// part of the name: compare against the bare version too.
const expectedVersion = expectedTag.replace(/^v/, '')
if (
  expectedTag &&
  installers.length &&
  !installers.some((name) => name.includes(expectedTag) || name.includes(expectedVersion))
) {
  failures.push(`no installer name carries the tag ${expectedTag} (found: ${installers.join(', ')})`)
}

const portable = join(root, 'src-tauri', 'target', 'release', 'harnesslite.exe')
if (!existsSync(portable)) failures.push('portable harnesslite.exe missing from target/release')
else {
  const bytes = statSync(portable).size
  if (bytes < 1_000_000) failures.push(`portable exe implausibly small (${bytes} bytes)`)
}

if (failures.length) {
  console.error('release asset check FAILED:\n  ' + failures.join('\n  '))
  process.exit(1)
}
console.log(`release assets OK (${installers.join(', ')})`)
