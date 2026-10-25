import { useEffect, useState, type ReactNode } from 'react'
import type { Artifact, LibraryItem } from '@shared/artifacts'
import { isImageName, type Attachment, type SharedFile } from '@shared/models'
import { DropdownMenu, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { libraryGroups } from '../../lib/library-groups'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import { useStore } from '../../store/context'
import { AttachmentImage, AttachmentViewer, attachmentSymbol, byteCount } from '../thread/Attachments'
import { downloadAction, downloadDetail } from '../thread/FileDownloadCard'
import { artifactShareItems, renameArtifact, TitleField } from './ArtifactParts'

/**
 * A chat's Library in its details panel (Grok Bot's): the artifacts its bot made and the files shared in it, newest
 * first, by day. Each opens the way it does in the chat.
 */
export function LibraryView({ botId, openArtifact }: { botId: string; openArtifact: (artifactId: string) => void }) {
  const tr = useTr()
  const store = useStore()
  const [items, setItems] = useState<LibraryItem[]>([])
  const [error, setError] = useState<string | null>(null)
  const [viewing, setViewing] = useState<Attachment | null>(null)
  useScreenViewed('library')
  // Read again when the chat gets an artifact card or a file.
  const shared = store.allEntries(botId).filter((e) => e.data.artifact || e.data.files?.length || e.data.attachments?.length).length
  useEffect(() => {
    let live = true
    store.artifacts.library(store.client, botId).then(
      (items) => live && (setItems(items), setError(null)),
      (e: unknown) => live && setError(e instanceof Error ? e.message : String(e)),
    )
    return () => {
      live = false
    }
  }, [store, store.client, botId, shared])
  return (
    <VStack gap={20}>
      {error ? <Text size="caption" tone="danger" role="alert">{error}</Text> : null}
      {items.length === 0 ? (
        <VStack gap={4} className="outlined-card library-empty">
          <Text size="callout" weight="semibold">{tr('Library is empty')}</Text>
          <Text size="callout" tone="secondary">{tr('Artifacts {name} makes and files shared in this chat show here.').replace('{name}', store.bots.get(botId)?.name ?? '')}</Text>
        </VStack>
      ) : (
        libraryGroups(items).map(({ group, items }) => (
          <section key={group} className="details-section">
            <Text className="details-section-title">{tr(group)}</Text>
            <VStack gap={6}>
              {items.map((item) =>
                item.kind === 'artifact' ? (
                  <ArtifactRow key={item.artifact.id} artifact={store.artifacts.byId.get(item.artifact.id) ?? item.artifact} open={() => openArtifact(item.artifact.id)} onError={setError} />
                ) : item.kind === 'file' ? (
                  <FileRow key={item.file.id} entryId={item.entryId} file={item.file} />
                ) : (
                  <AttachmentRow key={item.file.id} botId={botId} file={item.file} view={() => setViewing(item.file)} />
                ),
              )}
            </VStack>
          </section>
        ))
      )}
      <AttachmentViewer source={viewing && { file: viewing, botId }} close={() => setViewing(null)} />
    </VStack>
  )
}

/** One line of the Library: what it is, its name, and what can be done with it. */
function Row({ leading, children, trailing }: { leading: ReactNode; children: ReactNode; trailing?: ReactNode }) {
  return (
    <HStack gap={4} className="outlined-card library-row">
      <HStack gap={10} grow className="library-row-main">
        <span className="library-row-icon">{leading}</span>
        {children}
      </HStack>
      {trailing}
    </HStack>
  )
}

function ArtifactRow({ artifact, open, onError }: { artifact: Artifact; open: () => void; onError: (message: string | null) => void }) {
  const tr = useTr()
  const store = useStore()
  const [renaming, setRenaming] = useState(false)
  const menu = (
    <DropdownMenu
      className="icon-button library-row-menu"
      title={tr('More actions')}
      items={() => [
        { title: 'Open', icon: 'doc.text', action: open },
        { title: 'Rename artifact', icon: 'pencil', action: () => setRenaming(true) },
        ...artifactShareItems(store, artifact, onError),
      ]}
    >
      <Icon name="ellipsis" size={14} weight="medium" />
    </DropdownMenu>
  )
  const icon = <Icon name="chart.bar.doc.horizontal" size={16} color="var(--secondary)" />
  if (renaming) {
    return (
      <Row leading={icon} trailing={menu}>
        <TitleField
          initial={artifact.title}
          className="library-row-field"
          done={(title) => {
            setRenaming(false)
            renameArtifact(store, artifact, title, onError)
          }}
        />
      </Row>
    )
  }
  return (
    <Row leading={icon} trailing={menu}>
      <button className="library-row-open" onClick={open}>
        <Name name={artifact.title} detail={tr('Artifact')} />
      </button>
    </Row>
  )
}

/** A file the bot shared: saved like its card in the chat. */
function FileRow({ entryId, file }: { entryId: string; file: SharedFile }) {
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  return (
    <Row
      leading={<Icon name={attachmentSymbol(file.name)} size={16} color="var(--secondary)" />}
      trailing={<IconButton title={downloadAction(state)} icon={busy ? 'xmark' : 'arrow.down.to.line'} onClick={() => (busy ? store.fileDownloads.cancel(file.id) : void store.fileDownloads.save(store.client, entryId, file))} />}
    >
      <Name name={file.name} detail={downloadDetail(state, file)} failed={state?.kind === 'failed'} />
    </Row>
  )
}

/** A file the user sent: an image opens full size, like in the chat. */
function AttachmentRow({ botId, file, view }: { botId: string; file: Attachment; view: () => void }) {
  if (!isImageName(file.name)) {
    return (
      <Row leading={<Icon name={attachmentSymbol(file.name)} size={16} color="var(--secondary)" />}>
        <Name name={file.name} detail={byteCount(file.size)} />
      </Row>
    )
  }
  return (
    <Row leading={<AttachmentImage source={{ file, botId }} fit="icon" />}>
      <button className="library-row-open" onClick={view}>
        <Name name={file.name} detail={byteCount(file.size)} />
      </button>
    </Row>
  )
}

function Name({ name, detail, failed = false }: { name: string; detail: string; failed?: boolean }) {
  return (
    <VStack gap={1} shrink>
      <Text size="callout" truncate title={name}>{name}</Text>
      <Text size="caption2" tone={failed ? 'danger' : 'tertiary'} truncate>{detail}</Text>
    </VStack>
  )
}
