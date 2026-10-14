import { CardForm, CardSection } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader } from '../../components/Overlay'
import { font } from '../../lib/fonts'

const PRIVACY_POLICY = 'https://codync.dev/privacy'

/** What sharing usage analytics sends; the same text as the kit's and the terminal UI's, true to the privacy policy. */
const details = [
  { icon: 'chart.bar', title: 'Features you use', detail: "Opening the app, creating or deleting a bot, sending a message (how many attachments, never the files), running a routine or a voice call, installing a connector. Each comes with the app version, the platform, and the bot's agent and model." },
  { icon: 'exclamationmark.triangle', title: 'Crashes and errors', detail: "When Codync crashes or an agent fails to answer: the kind of error and where in Codync's own code it happened. Never the error's message; file paths are cut down to Codync's own files." },
  { icon: 'globe', title: 'Approximate location', detail: 'Country and city, looked up from your IP address. The IP address itself is never stored.' },
  { icon: 'person.crop.circle', title: 'Who it belongs to', detail: "Your Codync account when you're signed in, otherwise a random ID made on this device." },
  { icon: 'eye.slash', title: 'Never shared', detail: 'Your messages, prompts, code, files, file names or paths, bot names, credentials, or anything your agents produce.' },
  { icon: 'lock.shield', title: "How it's used", detail: 'Only to spot bugs sooner and see which features work and where people get stuck. Never sold, never used for ads or to track you across other apps and websites.' },
  { icon: 'power', title: 'Your choice', detail: 'Each iPhone and each computer has its own switch. Turning it off stops sending right away and drops anything not sent yet.' },
]

/** Settings → Privacy's info button: everything Share usage analytics covers. */
export function AnalyticsSheet() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--background)' }}>
      <ModalHeader title="Usage analytics" />
      <CardForm>
        <CardSection
          footer={
            <>
              See the{' '}
              <button style={{ color: 'var(--text)', textDecoration: 'underline' }} onClick={() => window.codync.app.openExternal(PRIVACY_POLICY)}>
                Privacy Policy
              </button>{' '}
              for details.
            </>
          }
        >
          {details.map((d) => (
            <div key={d.title} className="settings-row" style={{ alignItems: 'flex-start' }}>
              <span style={{ width: 20, display: 'flex', justifyContent: 'center', paddingTop: 2 }}>
                <Icon name={d.icon} size={15} color="var(--secondary)" />
              </span>
              <div className="settings-stack" style={{ flex: 1, gap: 2 }}>
                <span>{d.title}</span>
                <span style={{ ...font('footnote'), color: 'var(--secondary)' }}>{d.detail}</span>
              </div>
            </div>
          ))}
        </CardSection>
      </CardForm>
    </div>
  )
}
