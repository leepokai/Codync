import { useEffect, useState } from 'react'
import type { SharedFile } from '@shared/models'
import { analytics } from '../../store/analytics'
import { useStore } from '../../store/context'
import { SharedAudio, type AudioSnapshot, type Sound, type SoundEvents } from './shared-audio'

const IDLE: AudioSnapshot = { phase: 'idle', elapsed: 0, duration: 0 }

/** A voice note's player for its card: fetched once from the computer as the card shows (a tap then plays at
 *  once, and its length read off those bytes shows the bar and clock before the first play), stopped when the
 *  card goes. */
export function useSharedAudio(file: SharedFile, entryId: string) {
  const store = useStore()
  const [snapshot, setSnapshot] = useState<AudioSnapshot>(IDLE)
  const [audio] = useState(() => new SharedAudio({
    load: () => store.sharedFileData(file, entryId),
    open: (data, events) => audioElement(data, events),
    length: headerLength,
  }, setSnapshot))
  useEffect(() => () => audio.dispose(), [audio])
  // The sound is fetched once the computer is online: a chat shown before it connects reads nothing.
  const online = store.connection.kind === 'online'
  useEffect(() => {
    if (online) void audio.preload()
  }, [audio, online])
  return { snapshot, toggle: () => void play(audio), seek: (seconds: number) => audio.seek(seconds) }
}

/** Taps the card's control; a start from the beginning reports whether the note played, else which step broke. */
async function play(audio: SharedAudio) {
  const fromStart = audio.snapshot.phase === 'idle' || audio.snapshot.phase === 'failed'
  await audio.toggle()
  if (!fromStart) return
  const { phase } = audio.snapshot
  if (phase === 'playing') analytics.track({ event: 'voice_note_played', outcome: 'played' })
  if (phase === 'failed') analytics.track({ event: 'voice_note_played', outcome: 'failed', error: audio.failure })
}

function audioElement(data: Uint8Array, events: SoundEvents): Sound {
  const player = new Audio(URL.createObjectURL(new Blob([data as BlobPart])))
  player.onplay = () => events.playing(true)
  player.onpause = () => events.playing(false)
  player.ontimeupdate = player.ondurationchange = () => events.time(player.currentTime, Number.isFinite(player.duration) ? player.duration : 0)
  player.onended = () => events.ended()
  player.onerror = () => events.failed()
  return {
    play: () => player.play(),
    pause: () => player.pause(),
    seek: (seconds) => { player.currentTime = seconds },
    dispose: () => URL.revokeObjectURL(player.src),
  }
}

/** The sound's length from its headers: `preload = 'metadata'` reads only those, nothing plays. */
function headerLength(data: Uint8Array): Promise<number> {
  const player = new Audio()
  player.preload = 'metadata'
  return new Promise<number>((resolve) => {
    player.onloadedmetadata = () => resolve(Number.isFinite(player.duration) ? player.duration : 0)
    player.onerror = () => resolve(0)
    player.src = URL.createObjectURL(new Blob([data as BlobPart]))
  }).finally(() => URL.revokeObjectURL(player.src))
}
