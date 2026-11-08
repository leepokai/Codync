import { Icon } from '../../components/Icon'
import { HStack } from '../../components/Stack'
import { useTr } from '../../lib/use-language'

/** The pictures a channel can wear: the same SF Symbols the host accepts. */
export const CHANNEL_ICONS = [
  'bubble.left', 'book', 'hammer', 'globe', 'person.2', 'bolt', 'gearshape', 'photo', 'key', 'pin',
  'folder', 'calendar', 'clock', 'bell.badge', 'star', 'terminal', 'sparkles', 'shippingbox', 'brain', 'safari',
]

/** A row of channel pictures, the chosen one marked; the first clears it. */
export function ChannelIconPicker({ value, save }: { value: string; save: (icon: string) => Promise<boolean> }) {
  const tr = useTr()
  return (
    <HStack gap={4} wrap className="channel-icons" role="group" aria-label={tr('Channel picture')}>
      <button className="channel-icon" aria-pressed={!value} title={tr('Member faces')} onClick={() => void save('')}>
        <Icon name="person.2" size={14} />
      </button>
      {CHANNEL_ICONS.map((icon) => (
        <button key={icon} className={icon === value ? 'channel-icon chosen' : 'channel-icon'} aria-pressed={icon === value}
          title={icon} onClick={() => void save(icon)}>
          <Icon name={icon} size={14} />
        </button>
      ))}
    </HStack>
  )
}
