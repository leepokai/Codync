import { useEffect, useMemo, useRef, useState } from 'react'
import { SearchField } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Text } from '../../components/Text'
import { AnchoredPanel } from '../bots/parts'
import { currentZone, matchZones, timeZoneOptions, zoneLabel, zoneOffset } from './time-zones'
import './routines.css'

/** The routine's time zone: a button that opens every zone, filtered as you type. */
export function TimeZonePicker({ zone: saved, onChange }: { zone: string; onChange: (zone: string) => void }) {
  const zone = currentZone(saved)
  const anchor = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  return (
    <>
      <button
        ref={anchor}
        className="schedule-input zone-button"
        aria-label={`Time zone, ${zone}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen(true)}
      >
        <Text truncate grow>{zoneLabel(zone)}</Text>
        <Text tone="secondary">{zoneOffset(zone)}</Text>
        <Icon name="chevron.up.chevron.down" size={10} weight="semibold" color="var(--secondary)" />
      </button>
      <AnchoredPanel open={open} onClose={() => setOpen(false)} anchor={anchor}>
        <ZoneList
          zone={zone}
          pick={(picked) => {
            setOpen(false)
            onChange(picked)
          }}
        />
      </AnchoredPanel>
    </>
  )
}

/** The search box over the matching zones; ↑/↓ move, Return picks. */
function ZoneList({ zone, pick }: { zone: string; pick: (zone: string) => void }) {
  const zones = useMemo(() => timeZoneOptions(), [])
  const [query, setQuery] = useState('')
  const matches = useMemo(() => matchZones(zones, query), [zones, query])
  const [highlight, setHighlight] = useState(() => Math.max(0, zones.findIndex((z) => z.id === zone)))
  const input = useRef<HTMLInputElement>(null)
  const list = useRef<HTMLDivElement>(null)

  useEffect(() => input.current?.focus(), [])
  useEffect(() => {
    list.current?.querySelector(`[data-index="${highlight}"]`)?.scrollIntoView({ block: 'nearest' })
  }, [highlight, matches])

  return (
    <div
      className="zone-picker"
      onKeyDown={(e) => {
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
          e.preventDefault()
          setHighlight((h) => Math.min(Math.max(h + (e.key === 'ArrowDown' ? 1 : -1), 0), matches.length - 1))
        } else if (e.key === 'Enter' && matches[highlight]) {
          e.preventDefault()
          pick(matches[highlight].id)
        }
      }}
    >
      <SearchField
        value={query}
        prompt="Search time zones"
        inputRef={input}
        onChange={(q) => {
          setQuery(q)
          setHighlight(0)
        }}
      />
      <div ref={list} className="zone-list" role="listbox" aria-label="Time zones">
        {matches.map((z, i) => (
          <button
            key={z.id}
            data-index={i}
            role="option"
            aria-selected={z.id === zone}
            className={`menu-row${i === highlight ? ' highlight' : ''}`}
            onMouseEnter={() => setHighlight(i)}
            onClick={() => pick(z.id)}
          >
            <Text truncate grow>{zoneLabel(z.id)}</Text>
            <Text size="caption" tone="secondary">{z.offset}</Text>
            <span className={`check-slot${z.id === zone ? ' on' : ''}`}>
              <Icon name="checkmark" size={10} weight="semibold" />
            </span>
          </button>
        ))}
        {matches.length === 0 ? <Text size="callout" tone="secondary" className="zone-empty">No time zone matches “{query}”.</Text> : null}
      </div>
    </div>
  )
}
