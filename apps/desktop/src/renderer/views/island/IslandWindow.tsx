import { useRef } from 'react'
import { contentTop, isOpenMode, type IslandMode, type IslandTab } from '@shared/island'
import { Icon } from '../../components/Icon'
import { usePresence } from '../../components/Overlay'
import { useModel } from '../../lib/observable'
import { PINNED_LEAVE_MS, type IslandModel } from './island-model'
import { useCrossFade } from './island-motion'
import { Approval, Empty, Message, Notice, Overview, Wings } from './IslandViews'
import { UsageMeter, UsageView } from './IslandUsage'
import './island.css'

/** How long the views stay on screen while the shape folds back into the notch (its 0.34 s close). */
const FOLD_MS = 340

/** The notch island: a black shape that grows out of the notch and folds back into it. */
export function IslandWindow({ model }: { model: IslandModel }) {
  useModel(model)
  const { mode, frame } = model
  const open = isOpenMode(mode)
  const winged = mode === 'compact' || mode === 'peek'
  const closed = winged || mode === 'hidden'
  // What was on screen stays (fading) while the shape shrinks, so folding away is animated too.
  const lastOpen = useRef<IslandMode>(mode)
  const lastWings = useRef<IslandMode>(mode)
  if (open) lastOpen.current = mode
  if (winged) lastWings.current = mode
  const views = usePresence(open, FOLD_MS, false)
  const wings = usePresence(winged, FOLD_MS, false)
  return (
    <div
      className={`island ${model.growing ? 'growing' : 'shrinking'} ${model.screen.notch ? '' : 'edge'}`}
      data-mode={mode}
      style={{ width: frame.width, height: frame.height }}
      role={closed ? 'button' : undefined}
      aria-label={closed ? 'Show bots' : undefined}
      onClick={closed ? () => model.openNow() : undefined}
    >
      <span className="island-ear left" />
      <span className="island-ear right" />
      <div className="island-body" style={{ borderBottomLeftRadius: frame.radius, borderBottomRightRadius: frame.radius }}>
        {wings.mounted ? (
          <div className={`island-layer ${wings.shown ? '' : 'leaving'}`}>
            <Wings model={model} peek={lastWings.current === 'peek'} />
          </div>
        ) : null}
        {views.mounted ? (
          <div className={`island-layer island-views ${views.shown ? '' : 'leaving'}`}>
            <OpenViews model={model} mode={lastOpen.current} />
          </div>
        ) : null}
        {open && model.closing ? <span className="island-countdown" style={{ animationDuration: `${PINNED_LEAVE_MS}ms` }} /> : null}
      </div>
    </div>
  )
}

/** How long the view a tab switch replaced takes to fade while the next one fades in. */
const SWITCH_MS = 220

/**
 * The header and the view under it. Switching tabs (or an alert taking over) only cross-fades
 * the view: the shape keeps its size.
 */
function OpenViews({ model, mode }: { model: IslandModel; mode: IslandMode }) {
  const previous = useCrossFade(mode, SWITCH_MS)
  return (
    <>
      <Header model={model} mode={mode} />
      {(previous ? [previous, mode] : [mode]).map((m) => (
        <div key={m} className={`island-content ${m === previous ? 'leaving' : ''}`} style={{ top: contentTop(model.screen) }}>
          <View model={model} mode={m} />
        </div>
      ))}
    </>
  )
}

function View({ model, mode }: { model: IslandModel; mode: IslandMode }) {
  switch (mode) {
    case 'overview':
      return <Overview model={model} />
    case 'empty':
      return <Empty model={model} />
    case 'approval':
      return model.permission ? <Approval model={model} card={model.permission} /> : null
    case 'finished':
    case 'failed':
      return model.noticeBot ? <Notice model={model} bot={model.noticeBot} failed={mode === 'failed'} /> : null
    case 'message':
      return <Message model={model} />
    case 'usage':
      return <UsageView model={model} />
    default:
      return null
  }
}

const TABS: { tab: IslandTab; icon: string; label: string }[] = [
  { tab: 'bots', icon: 'house.fill', label: 'Bots' },
  { tab: 'message', icon: 'bubble.left.fill', label: 'Message a bot' },
  { tab: 'usage', icon: 'gauge.with.dots.needle.33percent', label: 'Usage' },
]

/** Tabs on the left of the notch; the working count, the usage meter and Codync on the right. */
function Header({ model, mode }: { model: IslandModel; mode: IslandMode }) {
  const selected = mode === 'message' || mode === 'usage' ? mode : mode === 'overview' || mode === 'empty' ? 'bots' : null
  const count = model.activeCount
  return (
    <div className="island-header">
      <div className="island-tabs">
        {TABS.map((t) => (
          <button key={t.tab} className={`island-tab ${selected === t.tab ? 'selected' : ''}`} title={t.label} aria-label={t.label} onClick={() => model.setTab(t.tab)}>
            <Icon name={t.icon} size={13} scaled={false} />
          </button>
        ))}
        <button className="island-tab" title="New bot" aria-label="New bot" onClick={() => model.newBot()}>
          <Icon name="plus" size={13} weight="semibold" scaled={false} />
        </button>
      </div>
      <div className="island-tabs">
        {count ? <span className="island-count">{`${count} working`}</span> : null}
        <UsageMeter model={model} />
        <button className="island-tab" title="Open Codync" aria-label="Open Codync" onClick={() => model.openApp()}>
          <Icon name="arrow.up.right.square" size={13} scaled={false} />
        </button>
      </div>
    </div>
  )
}
