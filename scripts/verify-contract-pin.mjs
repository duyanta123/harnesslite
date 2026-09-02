#!/usr/bin/env node
/**
 * Contract pin: the frontend mirror `src/console/protocol.ts` must agree with
 * the single source of truth `src-tauri/crates/hd-core/src/contract.rs`.
 *
 * Parsing by pattern is deliberate — both files are flat constant lists, so a
 * regex is the honest tool, and a parser that understands Rust would be a
 * second implementation of the contract to keep in step.
 */
import { readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
const rust = readFileSync(`${root}src-tauri/crates/hd-core/src/contract.rs`, 'utf8')
const ts = readFileSync(`${root}src/console/protocol.ts`, 'utf8')

const failures = []

const rustStrings = new Map(
  [...rust.matchAll(/pub const (\w+): &str = "((?:[^"\\]|\\.)*)";/g)].map((m) => [m[1], m[2]]),
)
const rustNumbers = new Map(
  [...rust.matchAll(/pub const (\w+): u32 = (\d+);/g)].map((m) => [m[1], Number(m[2])]),
)
const methodsMatch = rust.match(/pub const BRIDGE_METHODS: \[&str; \d+\] = \[([^\]]+)\]/)
if (!methodsMatch) failures.push('contract.rs: BRIDGE_METHODS array not found')
const rustMethods = methodsMatch
  ? [...methodsMatch[1].matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1])
  : []

const tsStrings = new Map(
  [...ts.matchAll(/export const (\w+) = '((?:[^'\\]|\\.)*)'/g)].map((m) => [m[1], m[2]]),
)
const tsNumbers = new Map(
  [...ts.matchAll(/export const (\w+) = (\d+)\n/g)].map((m) => [m[1], Number(m[2])]),
)
const methodsMatchTs = ts.match(/export const BRIDGE_METHODS = \[([^\]]+)\] as const/)
const tsMethods = methodsMatchTs
  ? [...methodsMatchTs[1].matchAll(/'((?:[^'\\]|\\.)*)'/g)].map((m) => m[1])
  : []

for (const [name, value] of rustStrings) {
  if (!(tsStrings.get(name) === value)) failures.push(`${name}: rust=${JSON.stringify(value)} ts=${JSON.stringify(tsStrings.get(name))}`)
}
for (const [name, value] of rustNumbers) {
  if (tsNumbers.get(name) !== value) failures.push(`${name}: rust=${value} ts=${tsNumbers.get(name)}`)
}
if (JSON.stringify(rustMethods) !== JSON.stringify(tsMethods)) {
  failures.push(`BRIDGE_METHODS differ:\n  rust=${JSON.stringify(rustMethods)}\n  ts=${JSON.stringify(tsMethods)}`)
}

if (failures.length) {
  console.error('contract pin FAILED:\n  ' + failures.join('\n  '))
  process.exit(1)
}
console.log(`contract pin OK (${rustStrings.size} strings, ${rustNumbers.size} numbers, ${rustMethods.length} bridge methods)`)
