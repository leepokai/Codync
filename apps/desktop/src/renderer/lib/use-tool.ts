import { useCallback, useEffect, useState } from 'react'
import type { HostSnapshot } from '@shared/ipc'
import type { ToolId, ToolState } from '@shared/tools'

const POLL_MS = 1500

/**
 * One bundled tool as this computer's host reports it, refreshed while the caller is mounted, with
 * a `call` for the tool's host methods (`toolStart`, `toolStop`, `comfyCancel`…). `host` is null
 * until this computer's host answers.
 */
export function useTool(id: ToolId, active = true, pollMs = POLL_MS) {
  const [host, setHost] = useState<HostSnapshot | null>(null)
  const [state, setState] = useState<ToolState | null>(null)

  useEffect(() => {
    let alive = true
    void window.codync.host.snapshot().then((s) => alive && setHost(s))
    const off = window.codync.host.onChange((s) => alive && setHost(s))
    return () => {
      alive = false
      off()
    }
  }, [])

  const local = host?.local ?? null
  const call = useCallback(
    <T = ToolState>(method: string, body: object = {}, timeoutMs = 15_000): Promise<T> => {
      if (!local) return Promise.reject(new Error('This computer’s host is not answering.'))
      return window.codync.host.call(local.baseURL, local.token, method, { id, ...body }, timeoutMs) as Promise<T>
    },
    [local, id],
  )

  useEffect(() => {
    if (!local || !active) return
    let alive = true
    const load = () => void call('toolStatus', {}, 5000).then((s) => alive && setState(s)).catch(() => {})
    load()
    const timer = setInterval(load, pollMs)
    return () => {
      alive = false
      clearInterval(timer)
    }
  }, [local, active, call, pollMs])

  return { host: local, hostKnown: host !== null, state, setState, call }
}
