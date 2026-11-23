import { useEffect, useState } from 'react'

/** How long `Path copied` (or the like) stands in after a copy. */
export const COPIED_MS = 1600

/** Whether something was just copied, and `copy`, which copies `text` and says so for a moment. */
export function useCopied(): [boolean, (text: string) => void] {
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const timer = setTimeout(() => setCopied(false), COPIED_MS)
    return () => clearTimeout(timer)
  }, [copied])
  return [copied, (text) => {
    window.codync.app.copy(text)
    setCopied(true)
  }]
}
