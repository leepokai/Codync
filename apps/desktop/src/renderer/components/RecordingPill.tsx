import { useEffect, useState } from 'react'
import { Spinner } from './Controls'
import { Icon } from './Icon'
import { LevelDots, type LevelSample } from './LevelDots'
import { HStack } from './Stack'
import { Text } from './Text'
import './recording-pill.css'
import { useTr } from '../lib/use-language'


/** m:ss since `startedAt`, updated every second. */
function useElapsed(startedAt: number) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(timer)
  }, [])
  const seconds = Math.max(0, Math.floor((now - startedAt) / 1000))
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

/**
 * Dictation in progress, in place of the mic: stop, the time since it started, and dots that
 * follow the microphone's level; `transcribing` once a recording waits for its transcript.
 * `color` null is the text color.
 */
export function RecordingPill({ startedAt, sample, onStop, transcribing = false, color = null, className = '' }: {
  startedAt: number
  sample: () => LevelSample
  onStop: () => void
  transcribing?: boolean
  color?: string | null
  className?: string
}) {
  const tr = useTr()
  const elapsed = useElapsed(startedAt)
  if (transcribing) {
    return (
      <HStack as="span" gap={6} className={`recording-pill transcribing ${className}`}>
        <Spinner size={12} />
        <Text tone="secondary">{tr(`Transcribing…`)}</Text>
      </HStack>
    )
  }
  return (
    <HStack as="span" gap={6} className={`recording-pill ${className}`}>
      <button type="button" className="recording-stop" aria-label="Stop dictating" title="Stop dictating" onClick={onStop}>
        <Icon name="stop.fill" size={11} scaled={false} />
      </button>
      <Text tabular tone="text">{elapsed}</Text>
      <LevelDots sample={sample} color={color} width={32} height={12} label="Listening" />
    </HStack>
  )
}
