/** One grounded answer, held only while the call is open. Suggestions are Google's original HTML. */
export interface SearchResult { html: string; text: string }

/** Joins transcription and grounding metadata for the same model turn. Transcription is not ordered with the other
 * server messages, so after `turnComplete` the voice's words alone can't say which answer they belong to: the next
 * turn starts when the user is heard or the next answer's model content arrives, whichever comes first. Words that
 * arrive before either stay with the answer that just finished. */
export class SearchTurn {
  result: SearchResult | null = null
  /** Turns that came with search suggestions. */
  answers = 0
  private text = ''
  private html: string | null = null
  private complete = false

  receive(content: Record<string, any>): SearchResult | null {
    const text = content.outputTranscription?.text
    const html = content.groundingMetadata?.searchEntryPoint?.renderedContent
    const next = this.complete && (content.modelTurn != null || typeof content.inputTranscription?.text === 'string')
    if (next) { this.text = ''; this.html = null; this.complete = false }
    if (typeof text === 'string') this.text += text
    if (typeof html === 'string' && html.trim()) {
      if (this.html === null) this.answers++
      this.html = html
    }
    if (next || typeof text === 'string' || typeof html === 'string') {
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
