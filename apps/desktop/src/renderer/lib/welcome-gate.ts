/**
 * Whether the welcome covers the window: before the first run's choice and, in a build with accounts, whenever no
 * one is signed in once the saved session has been checked (signing out comes back here). Builds without accounts
 * show it once.
 */
export function showsWelcome({ onboarded, configured, ready, signedIn }: { onboarded: boolean; configured: boolean; ready: boolean; signedIn: boolean }) {
  return !onboarded || (configured && ready && !signedIn)
}
