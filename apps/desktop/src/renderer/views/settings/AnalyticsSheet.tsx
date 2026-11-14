import { CardForm, CardSection } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, IconSlot, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { openLink } from '../../store/analytics'

const PRIVACY_POLICY = 'https://codync.dev/privacy'

/** What sharing usage analytics sends; the same text as the kit's and the terminal UI's, true to the privacy policy. */
const details = [
  { icon: 'chart.bar', title: 'Features you use', detail: "Opening the app and its screens, setting up and pairing (and whether you allow notifications), creating or deleting a bot, sending a message (how many attachments, never the files), running a routine or a voice call, installing a connector. Each comes with the app version, the platform, and the bot's agent and model." },
  { icon: 'exclamationmark.triangle', title: 'Crashes and errors', detail: "When Codync crashes or an agent fails to answer: the kind of error and where in Codync's own code it happened. Never the error's message; file paths are cut down to Codync's own files." },
  { icon: 'globe', title: 'Approximate location', detail: 'Country and city, looked up from your IP address. The IP address itself is never stored.' },
  { icon: 'person.crop.circle', title: 'Who it belongs to', detail: "Your Codync account when you're signed in, otherwise a random ID made on this device." },
  { icon: 'eye.slash', title: 'Never shared', detail: 'Your messages, prompts, code, files, file names or paths, bot names, credentials, or anything your agents produce.' },
  { icon: 'lock.shield', title: "How it's used", detail: 'Only to spot bugs sooner and see which features work and where people get stuck. Never sold, never used for ads or to track you across other apps and websites.' },
  { icon: 'power', title: 'Your choice', detail: 'Each iPhone and each computer has its own switch. Turning it off stops sending right away and drops anything not sent yet.' },
]

/** Settings → Privacy's info button: everything Share usage analytics covers. */
export function AnalyticsSheet() {
  const tr = useTr()
  return (
    <SheetPage>
      <ModalHeader icon="chart.bar" title={tr(`Usage analytics`)} />
      <CardForm>
        <CardSection
          footer={
            <>
              {tr('See the')}{' '}
              <button className="settings-link" onClick={() => openLink(PRIVACY_POLICY, 'privacy')}>
                {tr('Privacy Policy')}
              </button>{' '}
              {tr('for details.')}
            </>
          }
        >
          {details.map((d) => (
            <HStack key={d.title} align="flex-start" className="settings-row">
              <IconSlot width={20} className="settings-analytics-icon">
                <Icon name={d.icon} size={15} color="var(--secondary)" />
              </IconSlot>
              <VStack gap={2} grow className="settings-stack">
                <span>{tr(d.title)}</span>
                <Text size="footnote" tone="secondary">{tr(d.detail)}</Text>
              </VStack>
            </HStack>
          ))}
        </CardSection>
      </CardForm>
    </SheetPage>
  )
}
