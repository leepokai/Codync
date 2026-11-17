import { useEffect } from 'react'
import { BotAvatar } from '../components/Avatar'
import { IconButton } from '../components/Controls'
import { Icon } from '../components/Icon'
import { ModalHeader, Sheet, SheetPage, usePresence } from '../components/Overlay'
import { VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { useTr } from '../lib/use-language'
import { useScreenViewed } from '../store/analytics'
import { refKey, type BotRef, type RosterItem } from '../store/app-model'
import { StoreContext } from '../store/context'
import { LiveWork } from './thread/WorkRows'
import './all-work.css'

/**
 * Opens the view of all bots' work: in the roster's header only while some bot has a live or waiting item, with how
 * many items are worked on (no number at none).
 */
export function AllWorkButton({ shown, working, size, className, onClick }: { shown: boolean; working: number; size: number; className?: string; onClick: () => void }) {
  const presence = usePresence(shown, 200)
  if (!presence.mounted) return null
  return (
    <span className={`all-work-button ${presence.shown ? 'shown' : ''}`}>
      <IconButton title="Work" icon="tray.full" size={size} className={className} onClick={onClick} />
      {working > 0 ? <span className="all-work-count" aria-hidden>{working}</span> : null}
    </span>
  )
}

/**
 * Every bot that has live or waiting items, in one sheet: the bot as a heading, which opens its Details
 * (`openDetails`), over the rows its Details has for them, with the same actions. A row that needs the user opens
 * the bot's chat at that item's card (`openCard`); a row of an item a task has opens the bot's chat with that
 * thread in its inspector (`openThread`). It closes when the last item ends.
 */
export function AllWorkSheet({ open, bots, many, openDetails, openCard, openThread, onClose }: {
  open: boolean
  bots: RosterItem[]
  /** Several computers: a heading also names the bot's computer. */
  many: boolean
  openDetails: (ref: BotRef) => void
  openCard: (ref: BotRef, itemId: string) => void
  openThread: (ref: BotRef, itemId: string) => void
  onClose: () => void
}) {
  const empty = bots.length === 0
  useEffect(() => {
    if (open && empty) onClose()
  }, [open, empty, onClose])
  return (
    <Sheet open={open && !empty} onClose={onClose} width={480} maxHeight={640} dismissOnDim>
      <AllWork bots={bots} many={many} openDetails={openDetails} openCard={openCard} openThread={openThread} />
    </Sheet>
  )
}

function AllWork({ bots, many, openDetails, openCard, openThread }: { bots: RosterItem[]; many: boolean; openDetails: (ref: BotRef) => void; openCard: (ref: BotRef, itemId: string) => void; openThread: (ref: BotRef, itemId: string) => void }) {
  const tr = useTr()
  useScreenViewed('all_work')
  return (
    <SheetPage>
      <ModalHeader title={tr('Work')} />
      <VStack gap={20} className="all-work-list">
        {bots.map(({ ref, bot, computer, store }) => (
          <StoreContext.Provider key={refKey(ref)} value={store}>
            <VStack as="div" gap={8} data-all-work-bot={bot.name}>
              <button className="all-work-bot" title={tr('{name}, view details').replaceAll('{name}', () => bot.name)} onClick={() => openDetails(ref)}>
                <BotAvatar bot={bot} size={22} animated={false} />
                <Text size="body" weight="semibold" tone="text" truncate>{many ? `${bot.name} · ${computer.name}` : bot.name}</Text>
                <Icon name="chevron.right" size={10} weight="semibold" color="var(--tertiary)" />
              </button>
              <LiveWork bot={bot} plain openCard={(itemId) => openCard(ref, itemId)} openThread={(itemId) => openThread(ref, itemId)} />
            </VStack>
          </StoreContext.Provider>
        ))}
      </VStack>
    </SheetPage>
  )
}
