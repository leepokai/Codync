import type { Bot } from '@shared/models'
import { useRef } from 'react'
import { RowLabel, Switch } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { fill } from '../../lib/fill'
import { controlReady, screenAvailable, screenNotice } from '../../lib/screen-state'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import type { BotStore } from '../../store/bot-store'
import { useApp, useStore } from '../../store/context'
import { ComputerBadge } from '../ComputerBadge'
import { Notice } from '../screen/ScreenViewer'
import { screenComputer, useScreenSession, useVideo, useViewSize, useWindowShown } from '../screen/use-screen-session'
import './computer-tab.css'

/** The computer the bot lives on: how it is, a live look at its screen (watch the bot, click to take over), and whether
 * the bot may use it. This computer's own screen is right here, so it has no picture. */
export function ComputerTab({ bot }: { bot: Bot }) {
  const tr = useTr()
  const app = useApp()
  const store = useStore()
  useScreenViewed('bot_computer')
  return (
    <>
      <div className="outlined-card computer-tab-card">
        <HStack gap={8} className="computer-tab-header">
          <ComputerBadge computer={store.computer} size={20} />
          <Text weight="medium" truncate grow>{store.hostName}</Text>
          <Text size="caption" tone={store.shownConnection.kind === 'online' || store.isAsleep ? 'secondary' : 'attention'}>{tr(store.connectionLabel)}</Text>
        </HStack>
        {store !== app.local ? <ScreenPreview store={store} bot={bot} /> : null}
      </div>
      <ComputerUse bot={bot} />
    </>
  )
}

/** The screen, view only, while this tab and the window show; it lets go while the full viewer has it. */
function ScreenPreview({ store, bot }: { store: BotStore; bot: Bot }) {
  const tr = useTr()
  const app = useApp()
  const online = store.connection.kind === 'online'
  const available = online && screenAvailable(store.screen)
  const viewing = app.screenComputer === store.computer.id
  const shown = useWindowShown()
  const session = useScreenSession(store, available && !viewing && shown)
  const area = useRef<HTMLDivElement>(null)
  const video = useRef<HTMLVideoElement>(null)
  useVideo(video, session)
  useViewSize(area, session)
  const notice = viewing ? null : screenNotice(screenComputer(store), session, available, tr)
  const agentId = store.screen?.agentBot ?? null
  const agent = agentId ? store.bots.get(agentId) : undefined
  const live = session?.phase.kind === 'live'
  return (
    <div ref={area} className="computer-tab-screen">
      <video ref={video} className="screen-viewer-video" autoPlay muted playsInline hidden={!session?.video} />
      {notice ? <Notice {...notice} compact /> : null}
      {live ? (
        <button className="computer-tab-open" title={tr('Open the screen')} aria-label={tr('Open the screen')}
          onClick={() => app.openScreen(store.computer.id, agentId === bot.id ? bot.id : null)}>
          {agent ? <span className="computer-tab-agent">{fill(tr('{name} is using the computer'), { name: agent.name })}</span> : null}
          <span className="computer-tab-expand"><Icon name="arrow.up.left.and.arrow.down.right" size={11} weight="semibold" /></span>
        </button>
      ) : null}
    </div>
  )
}

/** Whether this bot may use the computer, switched right here (also in Settings). */
function ComputerUse({ bot }: { bot: Bot }) {
  const store = useStore()
  const screen = store.screen
  if (!screen) return null
  return (
    <div className="outlined-card details-switch">
      <RowLabel grow label="Use the computer" detail={controlReady(screen) ? null : 'On that computer, open Codync → Settings → Computer access to set it up first.'} />
      <HStack as="span" align="stretch" className="details-switch-control">
        <Switch label="Use the computer" on={bot.computer} disabled={store.isOffline} onChange={(on) => store.setComputer(bot, on)} />
      </HStack>
    </div>
  )
}
