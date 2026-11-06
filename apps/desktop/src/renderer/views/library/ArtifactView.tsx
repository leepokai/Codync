import { useEffect, useState } from 'react'
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
  const [error, setError] = useState<string | null>(null)
  const artifact = store.artifacts.byId.get(artifactId)
  const html = store.artifacts.html.get(artifactId)
  const failed = store.artifacts.failed.get(artifactId)
  const newest = artifact?.updatedAt
  useEffect(() => store.artifacts.load(store.client, botId, artifactId), [store, store.client, botId, artifactId, newest])
  useScreenViewed('artifact', artifactId)
  return (
    <div className="artifact-layer fade-in">
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
        {artifact ? <ArtifactTitle artifact={artifact} onError={setError} /> : null}
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
function ArtifactTitle({ artifact, onError }: { artifact: Artifact; onError: (message: string | null) => void }) {
  const tr = useTr()
  const store = useStore()
  const [editing, setEditing] = useState(false)
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
