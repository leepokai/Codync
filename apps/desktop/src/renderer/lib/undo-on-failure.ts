/** Waits for `action`; when it fails (refused, or the link gave up), `undo` takes back what it already showed before `fail` reports why. */
export async function undoOnFailure(action: Promise<unknown>, undo: (() => void) | undefined, fail: (message: string) => void): Promise<void> {
  try {
    await action
  } catch (error) {
    undo?.()
    fail(error instanceof Error ? error.message : String(error))
  }
}
