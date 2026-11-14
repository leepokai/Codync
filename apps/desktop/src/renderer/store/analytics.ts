import { useEffect } from 'react'
import type { Link, Screen } from '@shared/analytics'
import { AnalyticsQueue } from '../lib/analytics-queue'

/** This window's app-level analytics; `AppModel` connects it to this computer's host once it answers. */
export const analytics = new AnalyticsQueue()

/** A screen, sheet or tab is shown: once when it appears, and again when `key` changes. */
export function useScreenViewed(screen: Screen | null, key: unknown = null) {
  useEffect(() => {
    if (screen) analytics.track({ event: 'screen_viewed', screen })
  }, [screen, key])
}

/** Opens an outbound link in the browser, counted by what it leads to. */
export function openLink(url: string, link: Link) {
  analytics.track({ event: 'link_opened', link })
  window.codync.app.openExternal(url)
}
