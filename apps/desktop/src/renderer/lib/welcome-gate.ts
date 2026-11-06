/**
 * Whether the welcome covers the window: before the first run's choice and, in a build with accounts, whenever no
 * one is signed in once the saved session has been checked (signing out comes back here). Builds without accounts
 * show it once, and "Continue without an account" also puts it away.
 */
export function showsWelcome({ onboarded, configured, ready, signedIn, skipped }: {
  onboarded: boolean
  configured: boolean
  ready: boolean
  signedIn: boolean
  /** "Continue without an account" was chosen. */
  skipped: boolean
}) {
  return !onboarded || (configured && ready && !signedIn && !skipped)
}
