/** One grounded answer, held only while the call is open. Suggestions are Google's original HTML. */
export interface SearchResult { html: string; text: string }

/** Joins independently streamed transcription and grounding metadata for the same model turn. */
export class SearchTurn {
  result: SearchResult | null = null
  private text = ''
  private html: string | null = null
  private complete = true

  receive(content: Record<string, any>): SearchResult | null {
    const text = content.outputTranscription?.text
    const html = content.groundingMetadata?.searchEntryPoint?.renderedContent
    if (typeof text === 'string' || typeof html === 'string') {
      if (this.complete) { this.text = ''; this.html = null }
      this.complete = false
      if (typeof text === 'string') this.text += text
      if (typeof html === 'string' && html.trim()) this.html = html
      this.result = this.html ? { html: this.html, text: this.text } : null
    }
    if (content.turnComplete === true || content.interrupted === true) this.complete = true
    return this.result
  }
}

export function searchLink(value: string): string | null {
  try {
    const url = new URL(value)
    return url.protocol === 'https:' || url.protocol === 'http:' ? url.href : null
  } catch { return null }
}

/** No scripts, storage APIs, forms, or background network requests inside provider HTML. */
export const searchDocument = (html: string) => `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; base-uri 'none'; form-action 'none'"><meta name="viewport" content="width=device-width, initial-scale=1">${html}`
