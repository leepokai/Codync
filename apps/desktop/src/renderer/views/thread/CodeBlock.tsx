import { useEffect, useRef, useState } from 'react'
import { IconButton } from '../../components/Controls'
import { copyText } from '../../lib/image-clipboard'
import { analytics } from '../../store/analytics'
import { COPIED_MS } from './WorkThread'

/** A fenced code block: the code, and a corner button that copies it without the fences. */
export function CodeBlock({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  // Counts completed copies so re-clicking restarts the timer cleanly.
  const [copies, setCopies] = useState(0)
  const latest = useRef(0)
  const alive = useRef(true)

  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])

  useEffect(() => {
    if (copies === 0) return
    const timer = setTimeout(() => setCopied(false), COPIED_MS)
    return () => clearTimeout(timer)
  }, [copies])

  const copy = async () => {
    const attempt = ++latest.current
    if (!await copyText(text) || !alive.current || attempt !== latest.current) return
    analytics.track({ event: 'code_block_copied' })
    setCopied(true)
    setCopies((n) => n + 1)
  }

  return (
    <div className="md-code-block">
      <div className="md-code">
        <pre className="md-code-text">{text}</pre>
      </div>
      <IconButton
        className="md-code-copy"
        title={copied ? 'Copied' : 'Copy code'}
        icon={copied ? 'checkmark' : 'doc.on.doc'}
        size={24}
        onClick={() => void copy()}
      />
    </div>
  )
}
