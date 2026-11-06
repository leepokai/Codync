import { useState } from 'react'
import { Button, CardForm, CardSection, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ModalHeader, SheetPage } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { errorText, type MarketApp } from './market-models'
import { ErrorText, Field, ItemRow, MarketSection, ServiceLogo, WebLink } from './pieces'
import { usePlugins } from './plugins'
import { useTr } from '../../lib/use-language'

/**
 * An app's page: what it does, how to set it up (steps, a key, or its own sign-in), and example
 * prompts. Add connects it and starts the sign-in when it wants one.
 */
export function AppSheet({ app, added, onAdded, onError }: { app: MarketApp; added: boolean; onAdded: () => void; onError: (message: string) => void }) {
  const tr = useTr()
  const store = useStore()
  const plugins = usePlugins(store)
  const [values, setValues] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const installed = added || app.installed
  const missing = app.fields.some((f) => (f.required || f.url) && !(values[f.id] ?? '').trim())

  const add = async () => {
    setBusy(true)
    setError(null)
    try {
      await plugins.installApp(app, values)
      onAdded()
    } catch (e) {
      setError(errorText(e))
      onError(errorText(e))
    }
    setBusy(false)
  }

  const trailing = installed ? (
    <span title={tr('Added')} aria-label={tr('Added')} className="skill-added">
      <Icon name="checkmark" size={13} weight="semibold" />
    </span>
  ) : busy ? <Spinner /> : (
    <Button disabled={missing} onClick={() => void add()}>
      <Icon name="plus" size={12} />{app.setup === 'link' ? tr('Connect') : tr('Add app')}
    </Button>
  )

  return (
    <SheetPage>
      <ModalHeader icon={<ServiceLogo website={app.website} name={app.name} size={28} />} title={app.name} trailing={trailing} />
      <CardForm>
        <CardSection>
          <VStack gap={6}>
            <Text tone="text" weight="medium">{app.tagline}</Text>
            <Text tone="secondary">{app.description}</Text>
            {app.website ? <WebLink title={app.website} url={app.website} className="small truncate" /> : null}
          </VStack>
        </CardSection>
        {app.setup === 'local' && app.steps.length ? (
          <CardSection title={tr('Setup')} footer={tr('Runs on {name}.').replaceAll('{name}', store.hostName)}>
            <ol className="market-steps">
              {app.steps.map((step) => <li key={step}>{step}</li>)}
            </ol>
            {app.docsUrl ? <WebLink title={app.docsLabel ?? tr('Open the guide')} url={app.docsUrl} className="small" /> : null}
          </CardSection>
        ) : null}
        {app.setup === 'key' ? (
          <CardSection title={tr('Setup')} footer={tr('Keys are saved on {name} only.').replaceAll('{name}', store.hostName)}>
            {app.fields.map((field) => (
              <VStack key={field.id} gap={4}>
                <Field
                  label={field.label}
                  value={values[field.id] ?? ''}
                  secret={!field.url}
                  placeholder={field.placeholder ?? field.label}
                  onChange={(v) => setValues((old) => ({ ...old, [field.id]: v }))}
                />
                {field.hint ? <Text size="caption" tone="secondary">{field.hint}</Text> : null}
              </VStack>
            ))}
            {app.docsUrl ? <WebLink title={app.docsLabel ?? tr('Get a key')} url={app.docsUrl} className="small" /> : null}
          </CardSection>
        ) : null}
        {app.setup === 'link' || app.setup === 'open' ? (
          <CardSection title={tr('Setup')} footer={tr('Keys and sign-ins stay on {name}.').replaceAll('{name}', store.hostName)}>
            <Text tone="secondary">
              {app.setup === 'link' ? tr('{name} asks you to sign in to your own account in the browser.').replaceAll('{name}', app.name) : tr('Nothing to fill in here.')}
            </Text>
          </CardSection>
        ) : null}
        {app.prompts.length ? (
          <MarketSection title={tr('Try it')}>
            {app.prompts.map((p) => (
              <ItemRow
                key={p.id}
                title={p.text}
                subtitle={tr('Click to copy')}
                icon={<div className="market-tile"><Icon name="text.bubble" size={14} /></div>}
                onOpen={() => void navigator.clipboard.writeText(p.text)}
              />
            ))}
          </MarketSection>
        ) : null}
        {error ? <CardSection><ErrorText text={error} /></CardSection> : null}
      </CardForm>
    </SheetPage>
  )
}
