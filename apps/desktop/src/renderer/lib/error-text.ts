/** An error's message, also from a plain `{message}` object (errors that crossed the IPC bridge). */
export const errorText = (e: unknown) => (e instanceof Error ? e.message : typeof e === 'object' && e && 'message' in e ? String(e.message) : String(e))
