import { BotAvatar } from '../../components/Avatar'
import { CardSection, IconButton, Reveal } from '../../components/Controls'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModels } from '../../lib/observable'
import { refKey } from '../../store/app-model'
import { useApp } from '../../store/context'

/** Bots hidden from the sidebar on every computer, each with Unhide (the iPhone's Hidden bots). */
export function HiddenBotsSection() {
  const app = useApp()
  useModels([...app.stores.values()])
  const hidden = app.hiddenBots
  return (
    <Reveal show={hidden.length > 0}>
      <CardSection title="Hidden bots">
        {hidden.map(({ ref, bot, store }) => (
          <div key={refKey(ref)} className="settings-row">
            <BotAvatar bot={bot} size={28} animated={false} />
            <VStack grow className="settings-stack">
              <Text truncate>{bot.name}</Text>
              <Text size="caption" tone="tertiary">{store.hostName}</Text>
            </VStack>
            <IconButton title="Unhide" icon="eye" onClick={() => store.setHidden(bot, false)} />
          </div>
        ))}
      </CardSection>
    </Reveal>
  )
}
