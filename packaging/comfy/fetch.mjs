// Development only: puts the pinned ComfyUI portable (https://github.com/Comfy-Org/ComfyUI, GPL-3.0) into a
// directory, for the host's live tests and a dev host pointed at it with CODYNC_COMFY_DIR:
// node packaging/comfy/fetch.mjs <dir>. Packages don't ship it (NSIS can't hold 2.5 GB): the host downloads
// the same pin on its first start (host/src/tools/comfy_install.rs). version.json is the one place the
// version is named. The release's 7z is streamed to build/comfy, checked against the
// pinned size and SHA-256, and unpacked with 7za from the desktop app's 7zip-bin (Windows' own tar has no LZMA codec); only the program
// (ComfyUI/ and python_embeded/) is kept, with no models: those live in the user's data folder.
// Only Windows x64 gets the program (the NVIDIA portable also runs on the CPU). Any other target, or
// CODYNC_SKIP_COMFY=1 for a quick development package, gets an empty folder so packaging still works.
// Updating ComfyUI: change version.json (version, commit, asset, size, sha256 from the release page).
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { createReadStream, createWriteStream, existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { pipeline } from 'node:stream/promises'
import { Readable } from 'node:stream'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const pin = JSON.parse(readFileSync(join(here, 'version.json'), 'utf8'))
const out = process.argv[2]
if (!out) throw new Error('usage: fetch.mjs <dir>')

const stamp = join(out, '.pin')
const wanted = `${pin.version} ${pin.sha256}`
if (process.platform !== 'win32' || process.arch !== 'x64' || process.env.CODYNC_SKIP_COMFY === '1') {
  rmSync(out, { recursive: true, force: true })
  mkdirSync(out, { recursive: true })
  console.log(`comfy: not bundled (${process.platform}-${process.arch}${process.env.CODYNC_SKIP_COMFY === '1' ? ', skipped' : ''})`)
  process.exit(0)
}
if (existsSync(stamp) && readFileSync(stamp, 'utf8') === wanted) {
  console.log(`comfy ${pin.version} already in ${out}`)
  process.exit(0)
}

const sha256 = async (file) => {
  const hash = createHash('sha256')
  await pipeline(createReadStream(file), hash)
  return hash.digest('hex')
}
const cache = join(here, '../../build/comfy')
const archive = join(cache, pin.asset)
mkdirSync(cache, { recursive: true })
const valid = async () => existsSync(archive) && statSync(archive).size === pin.size && (await sha256(archive)) === pin.sha256
if (!(await valid())) {
  const url = `https://github.com/${pin.repo}/releases/download/${pin.version}/${pin.asset}`
  // GitHub's release CDN answers 5xx now and then: a few tries, a longer pause each time.
  let res
  for (let attempt = 1; ; attempt++) {
    res = await fetch(url)
    if (res.ok) break
    if (attempt === 4 || res.status < 500) throw new Error(`${url}: HTTP ${res.status}`)
    console.log(`${url}: HTTP ${res.status}, trying again`)
    await new Promise((resolve) => setTimeout(resolve, attempt * 15_000))
  }
  await pipeline(Readable.fromWeb(res.body), createWriteStream(archive))
  if (!(await valid())) {
    rmSync(archive, { force: true })
    throw new Error(`${pin.asset} doesn't match its pinned size and SHA-256`)
  }
}

const unpacked = join(cache, 'unpacked')
rmSync(unpacked, { recursive: true, force: true })
mkdirSync(unpacked, { recursive: true })
// 7zip-bin is a devDependency of the desktop app, which this script lives outside of.
const { path7za } = createRequire(join(here, '../../apps/desktop/package.json'))('7zip-bin')
execFileSync(path7za, ['x', '-y', `-o${unpacked}`, archive], { stdio: 'inherit' })
rmSync(out, { recursive: true, force: true })
mkdirSync(out, { recursive: true })
const root = join(unpacked, 'ComfyUI_windows_portable')
for (const part of ['ComfyUI', 'python_embeded']) renameSync(join(root, part), join(out, part))
writeFileSync(stamp, wanted)
rmSync(unpacked, { recursive: true, force: true })
console.log(`comfy ${pin.version} → ${out}`)
