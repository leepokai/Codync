import type { SharedFile } from '@shared/models'
import type { MenuItem } from '../../components/Menu'
import type { DownloadState } from '../../store/file-downloads'
import { trNow } from '../../lib/l10n.ts'
import type { AudioSnapshot } from './shared-audio'

/** A byte count the way Apple's `ByteCountFormatter(.file)` writes it: decimal units, no needless zeros. */
export function byteCount(bytes: number) {
  if (bytes <= 0) return 'Zero KB'
  if (bytes < 1000) return `${bytes} ${bytes === 1 ? 'byte' : 'bytes'}`
  const units = ['KB', 'MB', 'GB']
  const decimals = [0, 1, 2]
  let value = bytes / 1000
  let unit = 0
  while (unit < units.length - 1 && Number(value.toFixed(decimals[unit])) >= 1000) {
    value /= 1000
    unit++
  }
  return `${parseFloat(value.toFixed(decimals[unit]))} ${units[unit]}`
}

/** Seconds as `m:ss` (hours as `h:mm:ss`), for a sound's caption. */
export function clock(seconds: number) {
  const total = Math.floor(seconds)
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  const ss = String(s).padStart(2, '0')
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`
}

/** What the download button does now (also the Library's file rows). */
export function downloadAction(state: DownloadState | undefined) {
  return state?.kind === 'downloading' ? trNow('Cancel download') : state?.kind === 'failed' ? trNow('Retry download') : state?.kind === 'saved' ? trNow('Save again') : trNow('Download')
}

/** The download's menu item, titled for what it does now. */
export function downloadItem(state: DownloadState | undefined, download: () => void): MenuItem {
  return { title: downloadAction(state), icon: state?.kind === 'downloading' ? 'xmark' : 'arrow.down.to.line', action: download }
}

/** A card's actions, the first one what a click or Enter does: a voice note's play or pause, a picture's view, then the download's. */
export function cardActions({ state, audio, toggle, view, download }: {
  state: DownloadState | undefined
  audio?: AudioSnapshot
  toggle?: () => void
  view?: () => void
  download: () => void
}): MenuItem[] {
  const playing = audio?.phase === 'playing'
  return [
    ...(toggle ? [{ title: playing ? 'Pause' : 'Play', icon: playing ? 'pause.circle' : 'play.circle', action: toggle }] : []),
    ...(view ? [{ title: 'View', icon: 'eye', action: view }] : []),
    downloadItem(state, download),
  ]
}

type CardKeyAction = { kind: 'first' } | { kind: 'menu' } | { kind: 'seek'; seconds: number }

/**
 * What a key on a focused card does. Nothing while its menu is open: focus stays on the card, so Enter would
 * otherwise run the first action as well as the menu's highlighted one, and an arrow key move the recording.
 */
export function cardKeyAction(key: { key: string; shiftKey: boolean }, menuOpen: boolean, audio?: AudioSnapshot): CardKeyAction | undefined {
  if (menuOpen) return undefined
  if (key.key === 'Enter' || key.key === ' ') return { kind: 'first' }
  if (key.key === 'ContextMenu' || (key.shiftKey && key.key === 'F10')) return { kind: 'menu' }
  const seconds = seekKeyTarget(key.key, audio)
  return seconds === undefined ? undefined : { kind: 'seek', seconds }
}

/** Seconds an arrow key moves a voice note's playhead, as VoiceOver's adjust does on the phones. */
const SEEK_STEP = 5

/** Where an arrow key on a focused voice note moves its playhead (seconds), or undefined for any other key or a sound not loaded yet. */
function seekKeyTarget(key: string, audio: AudioSnapshot | undefined) {
  if (!audio || audio.duration <= 0) return undefined
  if (key === 'ArrowRight') return audio.elapsed + SEEK_STEP
  if (key === 'ArrowLeft') return audio.elapsed - SEEK_STEP
  return undefined
}

/** Whether a click on a card runs its first action: anywhere but its own buttons and the voice note's bar, which run theirs. */
export function clickRunsFirstAction(target: { closest(selector: string): unknown }) {
  return !target.closest('button, input')
}

/** The line under a shared file's name: its size, the download's progress, or what went wrong. */
export function downloadDetail(state: DownloadState | undefined, file: SharedFile) {
  switch (state?.kind) {
    case 'downloading': return `${byteCount(state.received)} of ${byteCount(file.size)}`
    case 'failed': return state.message
    case 'saved': return `${byteCount(file.size)} · Saved`
    default: return byteCount(file.size)
  }
}

/** A card's caption: a voice note that couldn't play, then the download's, then the sound's clock, then the file's. */
export function cardDetail(state: DownloadState | undefined, file: SharedFile, audio?: AudioSnapshot, tr = trNow) {
  if (audio?.phase === 'failed') return tr("Couldn't play. Click to retry.")
  if (state?.kind === 'downloading' || state?.kind === 'failed') return downloadDetail(state, file)
  if (audio && audio.duration > 0) return `${clock(audio.elapsed)} / ${clock(audio.duration)}`
  return downloadDetail(state, file)
}
