#!/usr/bin/env node
/**
 * One HTTPS GET with redirect following and (optionally) a file target.
 *
 * Node's global fetch does not read HTTP(S)_PROXY environment variables on
 * its own; on a network that needs the temporary proxy, set
 * NODE_USE_ENV_PROXY=1 (recent Node) or pre-populate runtime-cache/ by any
 * means that does — the checksum below is the real gate, not the transport.
 */
import { existsSync, readFileSync, writeFileSync } from 'node:fs'

export async function download(url, targetPath) {
  if (targetPath && existsSync(targetPath) && process.env.FORCE_DOWNLOAD !== '1') {
    // A cached artifact is still verified by the caller.
    return readFileSync(targetPath, 'utf8')
  }

  const response = await fetch(url, { redirect: 'follow' })
  if (!response.ok) {
    throw new Error(
      `${url} answered ${response.status}` +
        (process.env.HTTPS_PROXY
          ? ' (note: fetch ignores HTTP(S)_PROXY unless NODE_USE_ENV_PROXY=1)'
          : ''),
    )
  }

  if (!targetPath) return await response.text()

  const body = await response.arrayBuffer()
  writeFileSync(targetPath, Buffer.from(body))
  return readFileSync(targetPath, 'utf8')
}
