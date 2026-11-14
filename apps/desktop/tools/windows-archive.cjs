// NSIS's bundled decoder skips executable blocks using modern 7z ARM64/BCJ2 filters.
// Use its supported single-stream BCJ filter for both release and local Windows installers.
module.exports = (context) => {
  if (context.electronPlatformName === 'win32') process.env.ELECTRON_BUILDER_7Z_FILTER = 'BCJ'
}
