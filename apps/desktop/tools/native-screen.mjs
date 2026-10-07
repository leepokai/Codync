// Builds Codync Screen (Remote screen capture and input) into build/native/CodyncScreen.app for the
// Mac app to bundle, unsigned (electron-builder signs it; the cua-driver the Xcode target embeds
// keeps its own signature). Without it the app registers a launch agent pointing at a missing
// helper and Remote screen never starts.
import { execFileSync } from 'node:child_process'
import { cpSync, mkdirSync, rmSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

if (process.platform !== 'darwin') process.exit(0)
const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const repo = join(root, '../..')
const derived = join(repo, 'build/dd')
execFileSync('xcodebuild', [
  'build', '-project', join(repo, 'apps/Codync.xcodeproj'), '-scheme', 'Screen', '-configuration', 'Release',
  '-destination', 'generic/platform=macOS', '-derivedDataPath', derived, '-quiet',
  'CODE_SIGN_IDENTITY=-', 'CODE_SIGNING_REQUIRED=NO', 'CODE_SIGNING_ALLOWED=NO',
], { stdio: 'inherit' })
const out = join(root, 'build/native/CodyncScreen.app')
mkdirSync(dirname(out), { recursive: true })
rmSync(out, { recursive: true, force: true })
cpSync(join(derived, 'Build/Products/Release/CodyncScreen.app'), out, { recursive: true, verbatimSymlinks: true })
console.log('screen: built')
