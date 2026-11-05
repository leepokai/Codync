import { useEffect, useRef, useState } from 'react'
import { Button } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import { screenAvailable } from './screen-access'
import { screenNotice } from './screen-guidance'
import { useDocumentHidden, useScreenWindowOpen, useStageSize } from './screen-hooks'
import { thumbnailPhase } from './screen-mode'
import { ScreenNotice } from './ScreenNotice'
import { ScreenViewerModel } from './screen-viewer-model'
import './screen-viewer.css'

/** Until the computer says what its display is. */
const DEFAULT_RATIO = 16 / 10

/**
 * The details panel's Computer tab: a live, view-only preview of the computer's screen under a caption.
 * Clicking it opens the full Remote screen window; while that window is open the preview lets go of its
 * session (the window shows the screen), and it reconnects when the window closes.
 */
export function ScreenThumbnail({ computerId, name, open }: { computerId: string; name: string; open: () => void }) {
  const tr = useTr()
  const windowOpen = useScreenWindowOpen(computerId)
  const hidden = useDocumentHidden()
  const phase = thumbnailPhase({ windowOpen, hidden })
  const running = phase === 'running'
  const [model, setModel] = useState<ScreenViewerModel | null>(null)
  const card = useRef<HTMLDivElement>(null)
  const video = useRef<HTMLVideoElement>(null)

  // The session lives exactly as long as the preview is on show: leaving the tab, closing the panel, changing the bot, minimizing the app or opening the full window all end it.
  useEffect(() => {
    if (!running) return
    const next = new ScreenViewerModel(computerId, { viewOnly: true })
    setModel(next)
    next.start()
    const stop = () => next.stop()
    window.addEventListener('pagehide', stop)
    return () => {
      window.removeEventListener('pagehide', stop)
      stop()
      setModel(null)
    }
  }, [computerId, running])

  useModel(model)
  const session = model?.session ?? null
  const stream = session?.stream ?? null
  const display = model?.display ?? null
  const live = !!model && !!session && !!display && screenAvailable(model.screen)
  useStageSize(card, model)

  const notice = model ? screenNotice({ live, unreachable: model.unreachable, screen: model.screen, phase: session?.phase }, tr) : null
  // A "Reconnecting…" note sits over the last frame; any other notice replaces the video.
  const playing = live && (!notice || !!notice.dim)
  const frame = model?.input.viewport.frame
  // The element exists only while the video plays, which may come after the stream.
  useEffect(() => { if (video.current) video.current.srcObject = stream }, [stream, playing])
  const ratio = display && display.width > 0 && display.height > 0 ? display.width / display.height : DEFAULT_RATIO

  return (
    <VStack gap={10} className="screen-thumb-tab">
      <div ref={card} className="screen-thumb" style={{ aspectRatio: ratio }} onClick={(e) => { if (!(e.target as Element).closest('button')) open() }}>
        {playing && frame ? (
          <video ref={video} className="screen-thumb-video" autoPlay muted playsInline style={{ left: frame.x, top: frame.y, width: frame.width, height: frame.height }} />
        ) : null}
        {phase === 'window' ? (
          <VStack gap={10} align="center" className="screen-notice compact">
            <Icon name="display" size={22} color="var(--secondary)" />
            <Text size="caption" tone="secondary">{tr('Open in a window')}</Text>
            <Button kind="secondary" onClick={open}>{tr('Show')}</Button>
          </VStack>
        ) : notice ? (
          <ScreenNotice compact notice={notice} retry={() => model?.retry()} />
        ) : !playing && phase !== 'idle' ? (
          <ScreenNotice compact notice={{ text: tr('Connecting to the screen…') }} retry={() => {}} />
        ) : null}
        {playing && !notice ? (
          <button className="screen-thumb-open" type="button" onClick={open}>
            <Icon name="arrow.up.left.and.arrow.down.right" size={13} weight="semibold" color="#fff" />
            <Text size="callout" weight="medium">{tr('Open')}</Text>
          </button>
        ) : null}
      </div>
      <Text size="caption" tone="secondary" className="screen-thumb-caption">{tr('{name} screen').replaceAll('{name}', name)}</Text>
    </VStack>
  )
}
