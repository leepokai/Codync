// Which computer's Extensions show: the one asked for by name (a computer's heading in the roster, a deep link
// from inside a view), else the selected bot's computer, else this one, else the first online one.
// Pure, so the rail's follow-the-selection rule is tested without a renderer.

export type ExtensionsAsk = string | null | undefined

export function extensionsTarget(
  asked: ExtensionsAsk,
  selected: string | null | undefined,
  local: string | null | undefined,
  firstOnline: string | null | undefined,
): string | null {
  return asked || selected || local || firstOnline || null
}

/** The computer the rail leaves Extensions asking for: clicking its entry hands the page back to the selection. */
export function railExtensionsOf(asked: string | null, nextPage: string): string | null {
  return nextPage === 'extensions' ? null : asked
}
