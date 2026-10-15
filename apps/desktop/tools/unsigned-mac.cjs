// Unsigned Apple Silicon zip for a Mac that will view a remote computer.
// No notarization, no Codync Screen helper, and no on-device speech helper:
// those are only needed to capture or talk on this Mac. When
// build/native/codync-host exists (the macOS CI job), it is bundled so the
// app can start without Homebrew. Signing stays in electron-builder.yml.
const { existsSync } = require('node:fs')

const extraResources = [
  { from: 'resources/account-config.json', to: 'account-config.json' },
  { from: 'resources/tray', to: 'tray' },
]
if (existsSync('build/native/codync-host')) {
  extraResources.push({ from: 'build/native/codync-host', to: 'codync-host' })
}

module.exports = {
  appId: 'com.pokai.Codync',
  productName: 'Codync',
  copyright: '',
  directories: { output: 'dist', buildResources: 'resources' },
  files: ['out/**', 'package.json'],
  extraResources,
  protocols: [{ name: 'Codync', schemes: ['codync', 'com.pokai.Codync'] }],
  mac: {
    category: 'public.app-category.developer-tools',
    // electron-builder rejects a Mac icon under 512. `icon.png` is 256; the
    // unsigned build writes `icon-512.png` before packaging.
    icon: 'resources/icon-512.png',
    identity: null,
    hardenedRuntime: false,
    gatekeeperAssess: false,
    notarize: false,
    target: process.env.CODYNC_MAC_DMG === '1'
      ? [{ target: 'dmg', arch: ['arm64'] }, { target: 'zip', arch: ['arm64'] }]
      : [{ target: 'zip', arch: ['arm64'] }],
    artifactName: 'codync-macos-arm64.${ext}',
    extendInfo: {
      NSMicrophoneUsageDescription: 'Talk to your bots in a voice call.',
      NSSpeechRecognitionUsageDescription: 'Your words are transcribed on this Mac and sent to your bot as a message.',
    },
  },
}
