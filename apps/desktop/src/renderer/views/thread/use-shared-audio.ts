import { useEffect, useState } from 'react'
import type { SharedFile } from '@shared/models'
import { useStore } from '../../store/context'
import { SharedAudio, type AudioSnapshot, type Sound, type SoundEvents } from './shared-audio'

const IDLE: AudioSnapshot = { phase: 'idle', elapsed: 0, duration: 0 }

/** A voice note's player for its card: fetched once from the computer, stopped when the card goes. */
export function useSharedAudio(file: SharedFile, entryId: string) {
  const store = useStore()
  const [snapshot, setSnapshot] = useState<AudioSnapshot>(IDLE)
  const [audio] = useState(() => new SharedAudio({
    load: () => store.sharedFileData(file, entryId),
    open: (data, events) => audioElement(data, events),
  }, setSnapshot))
  useEffect(() => () => audio.dispose(), [audio])
  return { snapshot, toggle: () => void audio.toggle(), seek: (seconds: number) => audio.seek(seconds) }
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
