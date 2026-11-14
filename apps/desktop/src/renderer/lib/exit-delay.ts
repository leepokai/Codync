/** Hold exiting content until its animation ends; reopening cancels the pending removal. */
export function afterExit(duration: number, remove: () => void): () => void {
  const timer = setTimeout(remove, duration)
  return () => clearTimeout(timer)
}
