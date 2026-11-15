import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { searchDocument, searchLink, type SearchResult } from './search-result'
import './search-result.css'

/** Ephemeral alongside the live answer; never written to chat, memory, or browser storage. */
export function SearchResultView({ result }: { result: SearchResult }) {
  const tr = useTr()
  return <section className="call-search" aria-label={tr('Google Search')}>
    <Text size="caption" weight="semibold">{tr('Google Search')}</Text>
    {result.text ? <div className="call-search-answer">{result.text}</div> : null}
    <iframe title={tr('Google Search')} sandbox="allow-same-origin" referrerPolicy="no-referrer"
      srcDoc={searchDocument(result.html)} onLoad={(event) => {
        // Same-origin access is solely for native click handling; the frame cannot execute scripts.
        event.currentTarget.contentDocument?.addEventListener('click', (click) => {
          const target = click.target as Element | null
          const anchor = target?.closest?.('a[href]')
          if (!anchor) return
          click.preventDefault()
          const url = searchLink(anchor.getAttribute('href') ?? '')
          if (url) window.codync.app.openExternal(url)
        }, true)
      }} />
  </section>
}
