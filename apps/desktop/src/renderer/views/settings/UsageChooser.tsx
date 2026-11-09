import { useRef, useState, type KeyboardEvent } from 'react'
import type { Usage, UsageProvider, UsageService } from '@shared/models'
import { Switch } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { moveInOrder } from '../../lib/computer-roster'
import { fill } from '../../lib/fill.ts'
import { pointerDrag } from '../../lib/pointer-drag'
import { prefs, usePref } from '../../lib/prefs'
import { useSlide } from '../../lib/slide'
import { useTr } from '../../lib/use-language'
import { hasNoReading, pastMiddle, servicesIn } from '../../lib/usage-order.ts'
import { ProviderMascot } from '../usage/ProviderMascot'
import './settings.css'

/** What a mascot needs of an agent that has no reading. */
const unreadProvider = ({ id, name, registry }: UsageService): UsageProvider => ({ id, name, registry, windows: [], source: '', updatedAt: 0 })

/** The agents in the host's order, or the one here while a row is held or its new place is on its way to the host. */
function useUsageOrder(usage: Usage, onOrder: (ids: string[]) => Promise<void>) {
  const [order, setOrder] = useState<string[] | null>(null)
  const [held, setHeld] = useState<string | null>(null)
  // A drag's handlers are the ones from when it was pressed: they read the order here, not from that render.
  const moving = useRef<string[]>([])
  const shown = servicesIn(usage, order)
  const ids = shown.map((s) => s.id)
  const commit = (next: string[]) => {
    setOrder(next)
    // A later move keeps its own order on screen until its own answer.
    void onOrder(next).finally(() => setOrder((current) => (current === next ? null : current)))
  }
  const handlers = (id: string) => ({
    lift: () => { moving.current = ids; setHeld(id); setOrder(ids) },
    over: (target: string, y: number, box: DOMRect) => {
      const down = moving.current.indexOf(target) > moving.current.indexOf(id)
      if (!pastMiddle(down, y, box)) return
      moving.current = moveInOrder(moving.current, id, target)
      setOrder(moving.current)
    },
    drop: () => {
      setHeld(null)
      if (moving.current.join() === ids.join()) setOrder(null)
      else commit(moving.current)
    },
    cancel: () => { setHeld(null); setOrder(null) },
    step: (by: number) => {
      const target = ids[ids.indexOf(id) + by]
      if (target) commit(moveInOrder(ids, id, target))
    },
  })
  return { shown, held, handlers }
}

/** Which agents' limits this computer lists, and in what order. */
export function UsageChooser({ usage, onShown, onOrder }: {
  usage: Usage
  onShown: (id: string, shown: boolean) => void
  onOrder: (ids: string[]) => Promise<void>
}) {
  const tr = useTr()
  const { shown, held, handlers } = useUsageOrder(usage, onOrder)
  return (
    <VStack gap={10} align="stretch" className="fade-in">
      <div className="settings-usage-card settings-usage-chooser">
        {shown.map((service) => (
          <ChooserRow
            key={service.id}
            service={service}
            unread={hasNoReading(usage, service)}
            held={held === service.id}
            onShown={(on) => onShown(service.id, on)}
            drag={handlers(service.id)}
          />
        ))}
      </div>
      <Text tone="secondary" size="caption">{tr('On by default for the agents your bots run.')}</Text>
    </VStack>
  )
}

function ChooserRow({ service, unread, held, onShown, drag }: {
  service: UsageService
  unread: boolean
  held: boolean
  onShown: (on: boolean) => void
  drag: ReturnType<ReturnType<typeof useUsageOrder>['handlers']>
}) {
  const [style] = usePref(prefs.usageIconStyle)
  const tr = useTr()
  const row = useRef<HTMLDivElement>(null)
  useSlide(row)
  const grip = pointerDrag({
    start: drag.lift,
    over: (at, under) => {
      const target = under?.closest<HTMLElement>('[data-usage-service]')
      const id = target?.dataset.usageService
      if (target && id && id !== service.id) drag.over(id, at.y, target.getBoundingClientRect())
    },
    drop: drag.drop,
    cancel: drag.cancel,
  })
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return
    event.preventDefault()
    drag.step(event.key === 'ArrowUp' ? -1 : 1)
  }
  const moveLabel = fill(tr('Move {name}'), { name: service.name })
  return (
    <div ref={row} data-usage-service={service.id} className={`settings-usage-choice${held ? ' held' : ''}`}>
      <button className="settings-usage-grip" title={moveLabel} aria-label={moveLabel} onKeyDown={onKeyDown} {...grip}>
        <Icon name="line.3.horizontal" size={12} color="var(--secondary)" />
      </button>
      <Switch on={service.shown} onChange={onShown} className="settings-usage-switch">
        <HStack gap={8}>
          <ProviderMascot provider={unreadProvider(service)} size={18} style={style} />
          <Text tone="text" truncate>{service.name}</Text>
          {unread ? <Text tone="secondary" truncate>{tr('No reading yet')}</Text> : null}
        </HStack>
      </Switch>
    </div>
  )
}
