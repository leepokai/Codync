// Windows architectures share latest.yml; electron-updater selects the matching file by its name.
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parse, stringify } from 'yaml'

export function mergeWindowsUpdateFeeds(x64, arm64) {
  if (!x64.version || x64.version !== arm64.version) throw new Error('Windows update feed versions must match')
  for (const [arch, feed] of [['x64', x64], ['arm64', arm64]]) {
    const expected = `codync-windows-${arch}.exe`
    if (feed.files?.length !== 1 || feed.files[0].url !== expected || !feed.files[0].sha512) {
      throw new Error(`Windows ${arch} update feed must describe ${expected} with its SHA-512`)
    }
  }
  // Keep x64 path/sha512 for legacy clients that predate the files array.
  return { ...x64, files: [...x64.files, ...arm64.files] }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [x64Path, arm64Path, output] = process.argv.slice(2)
  if (!x64Path || !arm64Path || !output) throw new Error('usage: windows-update-feed.mjs <x64.yml> <arm64.yml> <output.yml>')
  const x64 = parse(readFileSync(x64Path, 'utf8'))
  const arm64 = parse(readFileSync(arm64Path, 'utf8'))
  const merged = mergeWindowsUpdateFeeds(x64, arm64)
  writeFileSync(output, stringify(merged))
}
