import { useEffect, useRef, useState } from 'react'
import { artifactFrameURL, type Artifact } from '@shared/artifacts'
import type { MenuItem } from '../../components/Overlay'
import type { BotStore } from '../../store/bot-store'
import type { ArtifactHTML } from '../../store/artifacts'
import { useTr } from '../../lib/use-language'
import './library.css'

/**
 * An artifact in a frame of its own origin (`main/artifacts.ts`): the whole artifact, or for a card its top, scaled down and
 * out of reach of the pointer.
 */
export function ArtifactFrame({ id, html, title, preview = false }: { id: string; html: ArtifactHTML; title: string; preview?: boolean }) {
  const [shown, setShown] = useState<number | null>(null)
  useEffect(() => {
    let live = true
    void window.codync.artifacts.show(id, html.bytes).then(() => live && setShown(html.updatedAt), console.error)
    return () => {
      live = false
    }
  }, [id, html])
  if (shown === null) return null
  return (
    <iframe
      className={preview ? 'artifact-frame preview' : 'artifact-frame'}
      src={artifactFrameURL(id, shown, preview)}
      title={title}
      // Its own origin already keeps it from the app; same-origin lets the artifact keep its own storage. A card opens nothing.
      sandbox={preview ? 'allow-scripts allow-same-origin' : 'allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox allow-downloads'}
      tabIndex={preview ? -1 : undefined}
      aria-hidden={preview || undefined}
      loading="lazy"
    />
  )
}

/** A title being edited in place: Return keeps it, Escape leaves it as it was. */
export function TitleField({ initial, className, done }: { initial: string; className: string; done: (title: string | null) => void }) {
  const tr = useTr()
  const [draft, setDraft] = useState(initial)
  const finished = useRef(false)
  const finish = (title: string | null) => {
    if (finished.current) return
    finished.current = true
    done(title && title !== initial ? title : null)
  }
  return (
    <input
      className={className}
      autoFocus
      value={draft}
      maxLength={200}
      aria-label={tr('Artifact title')}
      placeholder={tr('Artifact title')}
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => finish(draft.trim())}
      onKeyDown={(e) => {
        if (e.key === 'Enter') finish(draft.trim())
        if (e.key === 'Escape') finish(null)
      }}
    />
  )
}

const errorText = (error: unknown) => (error instanceof Error ? error.message : String(error))

/** Renames an artifact; what went wrong goes to `onError`. */
export function renameArtifact(store: BotStore, artifact: Artifact, title: string | null, onError: (message: string | null) => void) {
  if (title) store.artifacts.rename(store.client, artifact.botId, artifact.id, title).then(() => onError(null), (e: unknown) => onError(errorText(e)))
}

/** The artifact's share menu: Save as HTML (the native Save dialog) and Open in browser. */
export function artifactShareItems(store: BotStore, artifact: Artifact, onError: (message: string | null) => void): MenuItem[] {
  const run = (work: () => Promise<void>) => () => void work().then(() => onError(null), (e: unknown) => onError(errorText(e)))
  return [
    { title: 'Save as HTML', icon: 'arrow.down.to.line', action: run(() => store.artifacts.save(store.client, artifact)) },
    { title: 'Open in browser', icon: 'safari', action: run(() => store.artifacts.openInBrowser(store.client, artifact)) },
  ]
}
