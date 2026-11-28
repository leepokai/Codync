// Builds codync-speech (on-device speech recognition for calls, native/speech-macos) into
// build/native/codync-speech for the Mac app to bundle, unsigned (electron-builder signs it with the
// app's entitlements). Without it the packaged app has no on-device speech. The helper is the same for
// both environments, and always universal: it builds in seconds, so Codync Dev needs no shortcut.
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

if (process.platform !== 'darwin') process.exit(0)
const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const args = ['build', '-c', 'release', '--arch', 'arm64', '--arch', 'x86_64', '--package-path', join(root, 'native/speech-macos')]
execFileSync('swift', args, { stdio: 'inherit' })
const bin = execFileSync('swift', [...args, '--show-bin-path'], { encoding: 'utf8' }).trim()
const out = join(root, 'build/native')
mkdirSync(out, { recursive: true })
copyFileSync(join(bin, 'codync-speech'), join(out, 'codync-speech'))
console.log('speech: built')
