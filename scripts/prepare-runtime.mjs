#!/usr/bin/env node
/**
 * Stage a complete Node runtime into the installer's resources.
 *
 * The "full" edition bundles Node so the app works on machines without one.
 * The zip is downloaded once into runtime-cache/ (gitignored) and verified
 * against nodejs.org's SHASUMS256.txt before it is unpacked; unpacking uses
 * the system tar, which on Windows 10+ is bsdtar and reads zip archives.
 */
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath, URL } from 'node:url'
import { download } from './lib/download.mjs'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const version = process.env.HARNESSLITE_NODE_VERSION ?? 'v22.21.0'
const arch = 'win-x64'
const artifact = `node-${version}-${arch}`
const distBase = `https://nodejs.org/dist/${version}`

const cacheDir = join(root, 'runtime-cache')
const stagingDir = join(root, 'src-tauri', 'resources', 'runtime')
const zipPath = join(cacheDir, `${artifact}.zip`)

mkdirSync(cacheDir, { recursive: true })

// --- verify (reused cache included): checksum first, always ----------------
const shasUrl = `${distBase}/SHASUMS256.txt`
const shasums = await download(shasUrl)
const expected = shasums
  .split('\n')
  .map((line) => line.trim().split(/\s+/))
  .find(([, hash]) => hash === `${artifact}.zip`)
if (!expected) throw new Error(`SHASUMS256.txt has no entry for ${artifact}.zip`)

if (!existsSync(zipPath)) {
  console.log(`downloading ${distBase}/${artifact}.zip …`)
  await download(`${distBase}/${artifact}.zip`, zipPath)
}

const actual = createHash('sha256').update(readFileSync(zipPath)).digest('hex')
if (actual !== expected[0]) {
  rmSync(zipPath)
  throw new Error(`checksum mismatch for ${artifact}.zip: ${actual}`)
}

// --- unpack into the resource directory ------------------------------------
rmSync(stagingDir, { recursive: true, force: true })
mkdirSync(stagingDir, { recursive: true })
// bsdtar autodetects the zip; -J/-z flags are not needed. Windows' own
// bsdtar is named explicitly: a Git Bash / MSYS tar on PATH is GNU tar,
// which reads no zip at all.
const winTar = existsSync('C:/Windows/System32/tar.exe')
  ? 'C:/Windows/System32/tar.exe'
  : 'tar'
const unpacked = spawnSync(winTar, ['-xf', zipPath, '-C', stagingDir], { stdio: 'inherit' })
if (unpacked.status !== 0) throw new Error(`tar could not unpack ${artifact}.zip`)

// Flatten node-<v>-win-x64/ → runtime/node/ so the resource path is stable
// across Node versions.
rmSync(join(stagingDir, 'node'), { recursive: true, force: true })
const extracted = join(stagingDir, artifact)
const target = join(stagingDir, 'node')
spawnSync('cmd', ['/c', 'move', extracted, target], { stdio: 'inherit' })
if (!existsSync(join(target, 'node.exe'))) {
  throw new Error('the staged runtime has no node.exe; unpack layout changed')
}

writeFileSync(
  join(stagingDir, 'runtime.json'),
  `${JSON.stringify({ version, arch, sha256: actual, source: new URL(distBase).href }, null, 2)}\n`,
)
console.log(`staged ${version} (${arch}) → ${join('src-tauri', 'resources', 'runtime', 'node')}`)
