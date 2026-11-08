import { useEffect, useRef, useState } from 'react'
import { Button } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { ComposioConnectSheet, ComposioKeySheet } from './ComposioViews'
import { appConnected, errorText, market, type ComposioApp, type ComposioStatus } from './market-models'
import { AppLogo, FormSheet, ItemGrid, LoadMoreRow, MarketRow, MarketSection, SkeletonGrid, TileIcon } from './pieces'
import { usePlugins } from './plugins'

/**
 * The Apps tab's Composio section: set a key once, then connect apps. Connecting one adds it
 * on the computer like any other connector (kit `ComposioSection`).
 */
export function ComposioSection({ query }: { query: string }) {
  const tr = useTr()
  const store = useStore()
  const plugins = usePlugins(store)
  const [status, setStatus] = useState<ComposioStatus | null>(null)
  const [apps, setApps] = useState<ComposioApp[]>([])
  const [cursor, setCursor] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [settingUp, setSettingUp] = useState(false)
  const [connecting, setConnecting] = useState<ComposioApp | null>(null)
  const [reload, setReload] = useState(0)
  // Composio may repeat an app across pages; keep the one already shown.
  const seen = useRef(new Set<string>())

  useEffect(() => {
    let cancelled = false
    seen.current = new Set()
    setLoading(true)
    setCursor(null)
    void (async () => {
      const client = store.client
      if (!client) return
      try {
        const answer = await market.composioStatus(client)
        if (cancelled) return
        setStatus(answer)
        if (!answer.configured) {
          setApps([])
          return
        }
        const page = await market.composioApps(client, query)
        if (cancelled) return
        for (const app of page.items) seen.current.add(app.slug)
        setApps(page.items)
        setCursor(page.nextCursor)
        setError(null)
      } catch (e) {
        if (!cancelled) setError(errorText(e))
      } finally {
        if (!cancelled) setLoading(false)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [store, query, reload])

  const loadMore = async () => {
    const client = store.client
    if (!client || !cursor) return
    try {
      const page = await market.composioApps(client, query, cursor)
      setApps((shown) => [...shown, ...page.items.filter((app) => !seen.current.has(app.slug))])
      for (const app of page.items) seen.current.add(app.slug)
      setCursor(page.nextCursor)
      setError(null)
    } catch (e) {
      setError(errorText(e))
    }
  }

  return (
    <MarketSection title={tr('Apps')}>
      {status?.configured !== true ? (
        <MarketRow
          title="Composio"
          subtitle={tr('Gmail, Slack, GitHub, Notion and 1,000 more apps')}
          added={false}
          busy={status === null && loading}
          addLabel={tr('Set up')}
          icon={<TileIcon name="square.grid.3x3" />}
          onAdd={() => setSettingUp(true)}
        />
      ) : loading ? (
        <SkeletonGrid />
      ) : apps.length === 0 ? (
        <Text tone="secondary">
          {query ? tr('No apps match “{query}”.').replaceAll('{query}', query) : tr(`Couldn't load apps.`)}
        </Text>
      ) : (
        <>
          <ItemGrid>
            {apps.map((app) => (
              <MarketRow
                key={app.slug}
                title={app.name}
                subtitle={app.description ?? app.slug}
                added={appConnected(app)}
                addLabel="Connect"
                icon={<AppLogo url={app.logo} name={app.name} />}
                onAdd={() => setConnecting(app)}
              />
            ))}
          </ItemGrid>
          {cursor !== null ? <LoadMoreRow cursor={cursor} failed={error !== null} onLoad={() => void loadMore()} /> : null}
        </>
      )}
      {error && !loading ? <Text size="footnote" tone="danger">{tr(error)}</Text> : null}
      {status?.configured ? (
        <div>
          <Button kind="secondary" onClick={() => setSettingUp(true)}>
            <Icon name="key" size={11} />
            {tr('Composio settings')}
          </Button>
        </div>
      ) : null}
      <FormSheet open={settingUp} onClose={() => setSettingUp(false)}>
        <ComposioKeySheet
          status={status}
          done={(answer) => {
            setStatus(answer)
            setReload((n) => n + 1)
          }}
        />
      </FormSheet>
      <FormSheet open={connecting !== null} onClose={() => setConnecting(null)}>
        {connecting ? (
          <ComposioConnectSheet
            key={connecting.slug}
            app={connecting}
            done={() => {
              setReload((n) => n + 1)
              void plugins.refresh()
            }}
          />
        ) : null}
      </FormSheet>
    </MarketSection>
  )
}
