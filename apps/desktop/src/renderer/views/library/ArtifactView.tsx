import { useCallback, useEffect, useRef, useState } from 'react'
import type { Artifact } from '@shared/artifacts'
import { DropdownMenu, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import { useStore } from '../../store/context'
import { Composer } from '../thread/Composer'
import { MacChatButton } from '../thread/MacChatButton'
import { ArtifactFrame, artifactShareItems, renameArtifact, TitleField } from './ArtifactParts'

/**
 * An artifact full size over its chat (Grok Bot's artifact view): close, its title (click to rename) and share on top, and a
 * message box for its bot at the bottom. It shows the newest version, so an artifact the bot changes reloads here.
 */
export function ArtifactView({ botId, artifactId, close }: { botId: string; artifactId: string; close: () => void }) {
  const tr = useTr()
  const store = useStore()
  const back = useRef<HTMLDivElement>(null)
  const focusLayer = useCallback(() => back.current?.focus({ preventScroll: true }), [])
  // Take focus when the artifact opens: Escape and Tab then act on the artifact, not on the chat it covers.
  useEffect(() => { focusLayer() }, [focusLayer])
  const [error, setError] = useState<string | null>(null)
  const artifact = store.artifacts.byId.get(artifactId)
  const html = store.artifacts.html.get(artifactId)
  const failed = store.artifacts.failed.get(artifactId)
  const newest = artifact?.updatedAt
  useEffect(() => store.artifacts.load(store.client, botId, artifactId), [store, store.client, botId, artifactId, newest])
  useScreenViewed('artifact', artifactId)
  return (
    <div ref={back} tabIndex={-1} className="artifact-layer fade-in" onKeyDown={(e) => {
      // Escape leaves the artifact. A sheet, dialog or menu opened on top runs its own Escape first and
      // prevents this one (`useEscape`), and typing fields keep Escape for themselves.
      if (e.key !== 'Escape' || e.defaultPrevented) return
      const target = e.target as HTMLElement | null
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return
      e.stopPropagation()
      close()
    }}>
      <div className="top-chrome">
        <div className="top-chrome-row">
          <MacChatButton className="ringed" title={tr('Close artifact')} icon="xmark" direction={[-1, 0]} onClick={close} />
          <Spacer />
          {artifact ? (
            <DropdownMenu className="mac-chat-button ringed artifact-share" title={tr('Share artifact')} items={() => artifactShareItems(store, artifact, setError)}>
              <Icon name="square.and.arrow.up" size={16} weight="medium" />
            </DropdownMenu>
          ) : null}
        </div>
        {artifact ? <ArtifactTitle artifact={artifact} onError={setError} onDone={focusLayer} /> : null}
      </div>
      <div className="artifact-body">
        {html ? (
          <ArtifactFrame id={artifactId} html={html} title={artifact?.title ?? ''} />
        ) : failed ? (
          <VStack gap={6} className="artifact-state">
            <Text size="body" weight="semibold">{tr("Couldn't open the artifact")}</Text>
            <Text size="caption" tone="secondary">{failed}</Text>
          </VStack>
        ) : (
          <div className="artifact-state">
            <Spinner size={18} />
          </div>
        )}
      </div>
      <div className="composer-dock">
        {error ? <Text as="div" size="caption" tone="danger" role="alert" className="artifact-error">{error}</Text> : null}
        {/* A message here replies to the artifact's card, so its bot knows which artifact (`chat::replies`); an artifact
            published without one (a routine's) has nothing to reply to. */}
        {artifact?.entryId ? <Composer botId={botId} about={artifact} /> : null}
      </div>
    </div>
  )
}

/** The artifact's name in the top pill; clicking it edits the name in place. */
function ArtifactTitle({ artifact, onError, onDone }: { artifact: Artifact; onError: (message: string | null) => void; onDone: () => void }) {
  const tr = useTr()
  const store = useStore()
  const [editing, setEditing] = useState(false)
  // The name field holds the focus and goes away when the rename ends, saved or canceled with Escape. If the
  // focus then fell out of the artifact (the page body), take it back: the next Escape must still close the
  // artifact. Decide on the next frame, so a click into the message box or another control has taken its own
  // focus first; only a focus that is still on the page body falls back to the artifact.
  const wasEditing = useRef(false)
  useEffect(() => {
    const was = wasEditing.current
    wasEditing.current = editing
    if (!was || editing) return
    const frame = requestAnimationFrame(() => {
      if (!document.activeElement || document.activeElement === document.body) onDone()
    })
    return () => cancelAnimationFrame(frame)
  }, [editing, onDone])
  if (editing) {
    return (
      <TitleField
        initial={artifact.title}
        className="title-pill artifact-title-field"
        done={(title) => {
          setEditing(false)
          renameArtifact(store, artifact, title, onError)
        }}
      />
    )
  }
  return (
    <button className="title-pill" title={tr('Rename artifact')} onClick={() => setEditing(true)}>
      <HStack as="span" gap={7}>
        <Icon name="chart.bar.doc.horizontal" size={16} color="var(--secondary)" />
        <Text size="body" weight="medium" tone="text" truncate>{artifact.title}</Text>
      </HStack>
    </button>
  )
}
