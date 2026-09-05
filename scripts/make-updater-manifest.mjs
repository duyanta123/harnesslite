#!/usr/bin/env node
/**
 * Assemble the updater manifest (latest.json) from the signed bundle.
 *
 * `tauri build` (without --no-sign) leaves a `.sig` beside every updater
 * artifact: the minisign signature of that exact file. The updater feed is
 * then just the facts — which version, which URL, which signature — for the
 * platforms this release ships. The updater verifies the download against the
 * embedded public key AND this signature, so a mismatched pair is refused
 * before anything runs.
 *
 * Inputs (all defaults suit this repository's release workflow):
 *   --version   the release version, without the leading v   (tauri.conf.json)
 *   --nsis-dir  where the NSIS bundle landed                 (src-tauri/target/release/bundle/nsis)
 *   --repo      owner/name for the download URLs             (duyanta123/harnesslite)
 *   --tag       the release tag, with the leading v          (v<version>)
 *   --out       where to write latest.json                   (stdout when omitted)
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath, URL } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const conf = JSON.parse(readFileSync(join(root, 'src-tauri', 'tauri.conf.json'), 'utf8'))

const argv = process.argv.slice(2)
const arg = (name, fallback) => {
  const at = argv.indexOf(`--${name}`)
  return at !== -1 && argv[at + 1] !== undefined ? argv[at + 1] : fallback
}

const version = arg('version', conf.version)
const tag = arg('tag', `v${version}`)
const repo = arg('repo', 'duyanta123/harnesslite')
const nsisDir = resolve(arg('nsis-dir', join(root, 'src-tauri', 'target', 'release', 'bundle', 'nsis')))
const out = arg('out', null)

const installer = join(nsisDir, `HarnessLite_${version}_x64-setup.exe`)
const signaturePath = `${installer}.sig`
if (!existsSync(installer)) {
  throw new Error(`no installer at ${installer} — did the bundle step run?`)
}
if (!existsSync(signaturePath)) {
  throw new Error(
    `no signature at ${signaturePath} — the build must run WITH the signing key (not --no-sign) to produce an updater feed`,
  )
}

const signature = readFileSync(signaturePath, 'utf8').trim()
const url = `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(`HarnessLite_${version}_x64-setup.exe`)}`

const manifest = {
  version,
  pubkey: conf.plugins.updater.pubkey,
  platforms: {
    'windows-x86_64': { signature, url },
  },
}

const body = `${JSON.stringify(manifest, null, 2)}\n`
if (out) {
  mkdirSync(dirname(out), { recursive: true })
  writeFileSync(out, body)
  console.log(`wrote ${out} (windows-x86_64 → ${url})`)
} else {
  process.stdout.write(body)
}
