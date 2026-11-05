import { useEffect, useMemo, useState } from 'react'
import { Button, IconButton } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { errorText, type RoutineCalendarListing, type RoutineCalendarRun } from './routine'
import './routine-calendar.css'
import { useTr } from '../../lib/use-language'

/** A local day, as a number: Nov 4, 2026 is 20261104. */
const dayKey = (time: number) => {
  const d = new Date(time)
  return d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate()
}

interface DayMarks {
  done: number
  failed: number
  skipped: number
  upcoming: number
}

const emptyMarks = (): DayMarks => ({ done: 0, failed: 0, skipped: 0, upcoming: 0 })

/**
 * The bot's routines on a month: the runs that stood for each day, and the occurrences that
 * still run. The host counts within 60 days; one month at a time stays inside that.
 */
export function RoutineCalendarView({ botId }: { botId: string }) {
  const tr = useTr()
  const store = useStore()
  const [month, setMonth] = useState(() => {
    const now = new Date()
    return new Date(now.getFullYear(), now.getMonth(), 1)
  })
  const [selected, setSelected] = useState(() => new Date().setHours(0, 0, 0, 0))
  const [data, setData] = useState<RoutineCalendarListing | null>(null)
  const [error, setError] = useState<string | null>(null)

  const from = new Date(month.getFullYear(), month.getMonth(), 1).getTime()
  const to = new Date(month.getFullYear(), month.getMonth() + 1, 0, 23, 59, 59, 999).getTime()

  useEffect(() => {
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const tick = async () => {
      try {
        const result = await (await store.ready()).call<RoutineCalendarListing>('routineCalendar', { botId, from, to })
        if (!stopped) {
          setData(result)
          setError(null)
        }
      } catch (e) {
        if (!stopped && !store.isOffline) setError(errorText(e))
      }
      if (!stopped) timer = setTimeout(tick, 30000)
    }
    void tick()
    return () => {
      stopped = true
      clearTimeout(timer)
    }
  }, [botId, from, to])

  const nameOf = useMemo(() => new Map((data?.routines ?? []).map((r) => [r.id, r.name])), [data])

  const marks = useMemo(() => {
    const map = new Map<number, DayMarks>()
    const add = (key: number, patch: Partial<DayMarks>) => {
      const cell = map.get(key) ?? emptyMarks()
      for (const [key, count] of Object.entries(patch)) cell[key as keyof DayMarks] += count
      map.set(key, cell)
    }
    for (const run of data?.runs ?? []) {
      if (run.skipped) add(dayKey(run.at), { skipped: 1 })
      else if (run.status === 'succeeded') add(dayKey(run.at), { done: 1 })
      else if (run.status === 'failed' || run.status === 'interrupted') add(dayKey(run.at), { failed: 1 })
    }
    for (const upcoming of data?.scheduled ?? []) add(dayKey(upcoming.at), { upcoming: 1 })
    return map
  }, [data])

  const first = new Date(month.getFullYear(), month.getMonth(), 1)
  const days = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate()
  const cells: (Date | null)[] = [
    ...Array.from({ length: first.getDay() }, () => null),
    ...Array.from({ length: days }, (_, i) => new Date(month.getFullYear(), month.getMonth(), i + 1)),
  ]
  const todayKey = dayKey(Date.now())
  const selectedKey = dayKey(selected)

  const statusWord = (run: RoutineCalendarRun) => {
    if (run.skipped) return tr('Skipped')
    switch (run.status) {
      case 'succeeded':
        return tr('Ran')
      case 'failed':
        return tr('Failed')
      case 'interrupted':
        return tr('Interrupted')
      default:
        return run.status.charAt(0).toUpperCase() + run.status.slice(1)
    }
  }
  const toneOf = (run: RoutineCalendarRun) =>
    run.skipped ? 'skipped' : run.status === 'failed' || run.status === 'interrupted' ? 'failed' : 'done'

  // The selected day's moments, runs and to-come, in the order of the day.
  const moments = useMemo(() => {
    const runs = (data?.runs ?? []).filter((run) => dayKey(run.at) === selectedKey)
    const upcoming = (data?.scheduled ?? []).filter((s) => dayKey(s.at) === selectedKey)
    return [
      ...runs.map((run) => ({ at: run.at, routineId: run.routineId, label: statusWord(run), tone: toneOf(run) })),
      ...upcoming.map((s) => ({ at: s.at, routineId: s.routineId, label: tr('Upcoming'), tone: 'upcoming' })),
    ]
      .sort((a, b) => a.at - b.at)
      .map((m) => ({ ...m, name: nameOf.get(m.routineId) }))
  }, [data, selectedKey, nameOf, tr])

  const shift = (delta: number) => setMonth(new Date(month.getFullYear(), month.getMonth() + delta, 1))
  const goToday = () => {
    const now = new Date()
    setMonth(new Date(now.getFullYear(), now.getMonth(), 1))
    setSelected(now.setHours(0, 0, 0, 0))
  }

  return (
    <VStack gap={12} className="routine-calendar">
      <HStack align="center">
        <IconButton title={tr('Previous month')} icon="chevron.left" onClick={() => shift(-1)} />
        <Text size="body" weight="semibold" grow>
          {month.toLocaleDateString(undefined, { month: 'long', year: 'numeric' })}
        </Text>
        <IconButton title={tr('Next month')} icon="chevron.right" onClick={() => shift(1)} />
        <Button kind="secondary" onClick={goToday}>{tr('Today')}</Button>
      </HStack>
      {error ? <Text size="caption" tone="danger">{error}</Text> : null}
      {!data && !error ? <Text size="callout" tone="secondary">{tr('Loading calendar…')}</Text> : (
        <>
          <div className="cal-grid cal-head" aria-hidden>
            {['S', 'M', 'T', 'W', 'T', 'F', 'S'].map((d, i) => (
              <Text key={i} size="caption" tone="tertiary" className="cal-cell">{d}</Text>
            ))}
          </div>
          <div className="cal-grid" role="grid" aria-label={month.toLocaleDateString(undefined, { month: 'long', year: 'numeric' })}>
            {cells.map((day, i) =>
              day === null ? (
                <div key={i} className="cal-cell cal-blank" />
              ) : (
                <button
                  key={i}
                  role="gridcell"
                  className={`cal-cell cal-day${dayKey(day.getTime()) === todayKey ? ' today' : ''}${dayKey(day.getTime()) === selectedKey ? ' selected' : ''}`}
                  onClick={() => setSelected(day.getTime())}
                >
                  <Text size="caption">{day.getDate()}</Text>
                  <span className="cal-dots">
                    {dot(marks.get(dayKey(day.getTime())), 'upcoming')}
                    {dot(marks.get(dayKey(day.getTime())), 'done')}
                    {dot(marks.get(dayKey(day.getTime())), 'failed')}
                    {dot(marks.get(dayKey(day.getTime())), 'skipped')}
                  </span>
                </button>
              ),
            )}
          </div>
          <HStack gap={12} className="cal-legend">
            <span className="cal-legend-item"><span className="cal-dot upcoming" />{tr('Upcoming')}</span>
            <span className="cal-legend-item"><span className="cal-dot done" />{tr('Ran')}</span>
            <span className="cal-legend-item"><span className="cal-dot failed" />{tr('Failed')}</span>
            <span className="cal-legend-item"><span className="cal-dot skipped" />{tr('Skipped')}</span>
          </HStack>
          <VStack gap={6} className="cal-detail">
            <Text size="callout" weight="medium" tone="secondary">
              {new Date(selected).toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' })}
            </Text>
            {moments.length === 0 ? (
              <Text size="callout" tone="tertiary">{tr('Nothing runs this day.')}</Text>
            ) : (
              moments.map((m, i) => (
                <HStack key={i} gap={8} className="cal-moment">
                  <Text size="callout" tone="tertiary" className="cal-moment-time">
                    {new Date(m.at).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}
                  </Text>
                  <Text size="callout" grow>{m.name ?? '—'}</Text>
                  <Text size="callout" tone={m.tone === 'failed' ? 'danger' : m.tone === 'upcoming' ? 'text' : 'secondary'}>
                    {m.label}
                  </Text>
                </HStack>
              ))
            )}
          </VStack>
        </>
      )}
    </VStack>
  )
}

function dot(marks: DayMarks | undefined, kind: keyof DayMarks) {
  if (!marks || marks[kind] === 0) return null
  return <span key={kind} className={`cal-dot ${kind}`} title={`${marks[kind]}`} />
}
