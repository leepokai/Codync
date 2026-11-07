import { useState } from 'react'
import { showsAsAudio, showsAsImage, type Entry, type SharedFile } from '@shared/models'
import { useStore } from '../../store/context'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, IconButtonBase, useContextMenu } from '../../components/Overlay'
import { Spinner } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { AttachmentImage, AttachmentViewer, attachmentSymbol, type ImageSource } from './Attachments'
import { cardActions, cardDetail, cardKeyAction, clickRunsFirstAction, downloadAction, seekKeyTarget } from './file-card'
import type { AudioSnapshot } from './shared-audio'
import { useSharedAudio } from './use-shared-audio'
import './attachments.css'

export function FileDownloadCard({ entry, file }: { entry: Entry; file: SharedFile }) {
  return showsAsAudio(file) ? <AudioFileCard entry={entry} file={file} /> : <FileCard entry={entry} file={file} />
}

/** A voice note's card: its play state (and a spinner while it loads) in place of the file icon. */
function AudioFileCard({ entry, file }: { entry: Entry; file: SharedFile }) {
  const { snapshot, toggle, seek } = useSharedAudio(file, entry.id)
  return <FileCard entry={entry} file={file} audio={snapshot} toggle={toggle} seek={seek} icon={<AudioIcon snapshot={snapshot} toggle={toggle} />} />
}

/** A message's voice note, at the top of its bubble: the play button, the progress bar and the clock on one row. */
export function VoiceNotePlayer({ entry, file }: { entry: Entry; file: SharedFile }) {
  const { snapshot, toggle, seek } = useSharedAudio(file, entry.id)
  const stop = useFileStop(entry, file, snapshot, toggle, seek)
  return <>
    <div className="voice-note" {...stop.props} aria-label={`Voice note, ${stop.detail}`}>
      <HStack gap={10} aria-hidden>
        <AudioIcon snapshot={snapshot} toggle={toggle} />
        <AudioProgress snapshot={snapshot} seek={seek} />
        <Text as="div" size="caption2" tone={stop.failed ? 'danger' : 'secondary'} className="file-card-detail">{stop.detail}</Text>
      </HStack>
    </div>
    {stop.overlays}
  </>
}

/**
 * One screen-reader and keyboard stop, as on the phones: the card reads its name and caption, a click, Enter or
 * Space does its first action, the arrow keys move a voice note's playhead, and its menu (right-click, Shift-F10,
 * VoiceOver's Show menu) offers all of them. The buttons and bar inside are for the pointer only.
 */
function FileCard({ entry, file, audio, toggle, seek, icon }: {
  entry: Entry; file: SharedFile; audio?: AudioSnapshot; toggle?: () => void; seek?: (seconds: number) => void; icon?: React.ReactNode
}) {
  const stop = useFileStop(entry, file, audio, toggle, seek)
  return <>
    <div className="file-card" {...stop.props} aria-label={`${file.name}, ${stop.detail}`}>
      <VStack gap={12} aria-hidden>
        {showsAsImage(file) ? <button className="file-card-image" tabIndex={-1} onClick={stop.view}>
          <AttachmentImage source={stop.image} fit="thumbnail" />
        </button> : null}
        <HStack gap={12}>
          {icon ?? <Icon name={attachmentSymbol(file.name)} size={24} color="var(--secondary)" />}
          <VStack grow>
            <Text as="div" size="subheadline" weight="semibold" truncate title={file.name}>{file.name}</Text>
            <Text as="div" size="caption2" tone={stop.failed ? 'danger' : 'secondary'} className="file-card-detail">{stop.detail}</Text>
            {audio && seek ? <AudioProgress snapshot={audio} seek={seek} /> : null}
          </VStack>
          <IconButtonBase title={downloadAction(stop.state)} icon={stop.busy ? 'xmark' : 'arrow.down.to.line'} size={36} tabIndex={-1} onClick={stop.download} />
        </HStack>
      </VStack>
    </div>
    {stop.overlays}
  </>
}

/** A shared file's keyboard and screen-reader stop: its caption, its actions with their menu, and the picture viewer. */
function useFileStop(entry: Entry, file: SharedFile, audio?: AudioSnapshot, toggle?: () => void, seek?: (seconds: number) => void) {
  const store = useStore()
  const state = store.fileDownloads.states.get(file.id)
  const busy = state?.kind === 'downloading'
  const [viewing, setViewing] = useState(false)
  const menu = useContextMenu()
  const image: ImageSource = { file, entryId: entry.id }
  const view = () => setViewing(true)
  const download = () => store.fileDownloads.toggle(store.client, entry.id, file)
  const actions = () => cardActions({ state, audio, toggle, view: showsAsImage(file) ? view : undefined, download })
  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return
    const seconds = seekKeyTarget(e.key, audio)
    if (seek && seconds !== undefined) {
      e.preventDefault()
      return seek(seconds)
    }
    const action = cardKeyAction(e, menu.open)
    if (!action) return
    e.preventDefault()
    if (action === 'first') actions()[0].action()
    else {
      const { left, bottom } = e.currentTarget.getBoundingClientRect()
      menu.openAt({ x: left, y: bottom })
    }
  }
  return {
    state,
    busy,
    failed: state?.kind === 'failed' || audio?.phase === 'failed',
    detail: cardDetail(state, file, audio),
    image,
    view,
    download,
    props: {
      role: 'button',
      tabIndex: 0,
      onClick: (e: React.MouseEvent) => clickRunsFirstAction(e.target as Element) && actions()[0].action(),
      onKeyDown,
      onContextMenu: menu.onContextMenu,
    },
    overlays: <>
      <AnchoredMenu open={menu.open} onClose={menu.close} point={menu.point} items={actions} />
      <AttachmentViewer source={viewing ? image : null} close={() => setViewing(false)} />
    </>,
  }
}

/** The play/pause button, or the spinner while the sound comes over, in the file icon's 24 px slot. */
function AudioIcon({ snapshot, toggle }: { snapshot: AudioSnapshot; toggle: () => void }) {
  return <div className="file-card-play">
    {snapshot.phase === 'loading'
      ? <Spinner size={18} />
      : <IconButtonBase title={snapshot.phase === 'playing' ? 'Pause' : 'Play'} icon={snapshot.phase === 'playing' ? 'pause.circle' : 'play.circle'} size={36} tabIndex={-1} onClick={toggle} />}
  </div>
}

/** The bar beside or under the caption: how much of a loaded voice note has played; click or drag it to move there. */
function AudioProgress({ snapshot, seek }: { snapshot: AudioSnapshot; seek: (seconds: number) => void }) {
  if (snapshot.duration <= 0) return null
  return <input type="range" className="file-card-progress" tabIndex={-1} min={0} max={snapshot.duration} step="any"
    value={snapshot.elapsed} style={{ '--played': snapshot.elapsed / snapshot.duration } as React.CSSProperties}
    onChange={(e) => seek(Number(e.target.value))} />
}
