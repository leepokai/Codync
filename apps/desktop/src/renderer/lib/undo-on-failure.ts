/** Waits for `action` and resolves to whether it went through; when it fails (refused, or the link gave up), `undo` takes back what it already showed before `fail` reports why. */
export async function undoOnFailure(action: Promise<unknown>, undo: (() => void) | undefined, fail: (message: string) => void): Promise<boolean> {
  try {
    await action
    return true
  } catch (error) {
    undo?.()
    fail(error instanceof Error ? error.message : String(error))
    return false
  }
}
