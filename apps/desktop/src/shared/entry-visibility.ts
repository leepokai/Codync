/** Scheduled prompts and provisional background output remain in sync for revision ordering. */
export function isInternalEntry(entry: { kind?: string; data: { internal?: boolean; from?: string } }): boolean {
  return entry.data.internal === true || entry.data.from === 'proactive'
}
