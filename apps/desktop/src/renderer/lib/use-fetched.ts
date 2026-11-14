import { useEffect, useState, type DependencyList } from 'react'

/**
 * What `load` resolves to, null while it loads (or when it has nothing to load); `error` says why it failed. An answer
 * that arrives after `deps` changed is dropped.
 */
export function useFetched<T>(load: () => Promise<T> | null, deps: DependencyList): { fetched: T | null; error: string | null } {
  const [fetched, setFetched] = useState<T | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    const loading = load()
    if (!loading) return
    let current = true
    loading.then(
      (value) => {
        if (current) setFetched(value)
      },
      (reason: unknown) => {
        console.warn('load failed', reason)
        if (current) setError(reason instanceof Error ? reason.message : String(reason))
      },
    )
    return () => {
      current = false
    }
  }, deps)
  return { fetched, error }
}
