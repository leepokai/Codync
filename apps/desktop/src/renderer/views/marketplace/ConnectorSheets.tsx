import { useEffect, useRef, useState } from 'react'
import { CardForm, CardSection, ChoiceList, IconButton, SegmentedChoice, Spinner } from '../../components/Controls'
import { ModalHeader, SheetPage, useDismiss } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { errorText, market, needsSignIn, type InstalledConnector, type MarketConnector, type MarketInput } from './market-models'
import { ErrorText, Field, WebLink } from './pieces'
import { usePlugins } from './plugins'
import { useTr } from '../../lib/use-language'

export function InstallConnectorSheet({ item, requestId, done }: { item: MarketConnector; requestId?: string; done: () => void }) {
  const store = useStore()
  const plugins = usePlugins(store)
  const dismiss = useDismiss()
  const [optionId, setOptionId] = useState(item.options[0]?.id ?? '')
  const [values, setValues] = useState<Record<string, string>>({})
  const [installed, setInstalled] = useState<InstalledConnector | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const started = useRef(false)
  const option = item.options.find((o) => o.id === optionId) ?? item.options[0]
  const value = (input: MarketInput) => values[input.name] ?? ''
  // After a try that failed, Add with nothing typed tries the connector it saved again.
  const retrying = installed !== null && !Object.values(values).some((v) => v !== '')

  const install = async () => {
    if (!option) return
    setSaving(true)
    setError(null)
    try {
      const client = await store.ready()
      // The host keeps the connector a try saved unless new values or another way to run it come in.
      const c = await plugins.installConnector(item, option.id, values)
      setInstalled(c)
      setValues({})
      if (needsSignIn(c)) await plugins.signIn(c.id)
      if (requestId) await market.finishConnectionRequest(client, requestId, { connectorId: c.id })
      else await market.verifyConnector(client, c.id)
      await plugins.refresh()
      done()
      dismiss()
    } catch (e) {
      setError(errorText(e))
    }
    setSaving(false)
  }

  useEffect(() => {
    if (started.current) return
    started.current = true
    if (item.options.length === 1 && item.options[0].inputs.length === 0) void install()
  }, [])

  const tr = useTr()
  const onHost = (template: string) => tr(template).replaceAll('{name}', store.hostName)
  const missing = !option || option.inputs.some((i) => i.required && !(values[i.name] ?? i.default ?? ''))
  return (
    <SheetPage>
      <ModalHeader
        icon="point.3.connected.trianglepath.dotted"
        title={tr(`Add connector`)}
        trailing={saving ? <Spinner /> : <IconButton title="Add" icon="plus" disabled={missing && !retrying} onClick={() => void install()} />}
      />
      <CardForm>
        <CardSection>
          <VStack gap={6} className="market-card-header">
            <Text size="title3" weight="semibold">{item.title}</Text>
            {item.description ? <Text tone="secondary">{item.description}</Text> : null}
            {item.website && URL.canParse(item.website) ? (
              <WebLink title={item.website} url={item.website} className="small truncate" />
            ) : null}
          </VStack>
        </CardSection>
        {item.options.length > 1 ? (
          <CardSection title="Runs">
            <ChoiceList
              selection={optionId}
              onChange={setOptionId}
              options={item.options.map((o) => ({
                id: o.id,
                label: o.kind === 'remote'
                  ? tr('Hosted by {name}').replaceAll('{name}', item.title)
                  : tr('On {name} ({kind})').replaceAll('{name}', store.hostName).replaceAll('{kind}', o.kind),
              }))}
            />
          </CardSection>
        ) : null}
        {option ? (
          <CardSection title="Setup" footer={onHost('Keys are saved on {name} only.')}>
            {option.inputs.length === 0 ? (
              <Text tone="secondary">
                {option.kind === 'remote'
                  ? tr('No keys needed here. If {name} wants you to sign in, its sign-in page opens next.').replaceAll('{name}', item.title)
                  : tr(`No setup needed.`)}
              </Text>
            ) : null}
            {option.inputs.map((input) => (
              <VStack key={input.name} gap={4}>
                <Field
                  label={input.name}
                  value={value(input)}
                  secret={input.secret}
                  placeholder={input.secret ? input.name : (input.default ?? input.placeholder ?? input.name)}
                  onChange={(v) => setValues((old) => ({ ...old, [input.name]: v }))}
                />
                {input.description ? (
                  <Text size="caption" tone="secondary">{input.description + (input.required ? '' : tr(' (optional)'))}</Text>
                ) : null}
              </VStack>
            ))}
          </CardSection>
        ) : null}
        {error ? <CardSection><ErrorText text={error} /></CardSection> : null}
      </CardForm>
    </SheetPage>
  )
}

type Mode = 'command' | 'url' | 'config'

export function CustomConnectorSheet() {
  const store = useStore()
  const tr = useTr()
  const plugins = usePlugins(store)
  const dismiss = useDismiss()
  const [name, setName] = useState('')
  const [mode, setMode] = useState<Mode>('command')
  const [target, setTarget] = useState('')
  const [envText, setEnvText] = useState('')
  const [config, setConfig] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const ready = mode === 'config' ? config.trim() !== '' : name.trim() !== '' && target.trim() !== ''

  const onHost = (template: string) => tr(template).replaceAll('{name}', store.hostName)
  const footer = {
    command: onHost("Runs on {name} in the bot's project folder. Quotes work like in a shell."),
    url: 'A remote MCP server, streamable HTTP or SSE.',
    config: onHost('Paste the MCP config from a README or another app (Claude, Cursor, VS Code). Every server in it is added. Saved on {name} only.'),
  }[mode]

  const save = async () => {
    setSaving(true)
    setError(null)
    const remote = mode === 'url'
    const pairs: Record<string, string> = {}
    for (const line of envText.split('\n')) {
      const at = line.indexOf(remote ? ':' : '=')
      if (at < 0) continue
      const key = line.slice(0, at).trim()
      if (key) pairs[key] = line.slice(at + 1).trim()
    }
    try {
      const added = mode === 'config'
        ? await plugins.importConnectors(config)
        : [await plugins.addConnector({ name, command: remote ? null : target, url: remote ? target : null, env: remote ? {} : pairs, headers: remote ? pairs : {} })]
      for (const c of added) if (needsSignIn(c)) await plugins.signIn(c.id)
      dismiss()
    } catch (e) {
      setError(errorText(e))
    }
    setSaving(false)
  }

  return (
    <SheetPage>
      <ModalHeader icon="point.3.connected.trianglepath.dotted" title={tr(`Custom connector`)} trailing={saving ? <Spinner /> : <IconButton title="Add" icon="plus" disabled={!ready} onClick={() => void save()} />} />
      <CardForm>
        <CardSection footer={footer}>
          <SegmentedChoice selection={mode} onChange={setMode} options={[{ id: 'command', label: 'Command' }, { id: 'url', label: 'URL' }, { id: 'config', label: 'Config' }]} />
          {mode === 'config' ? <Field value={config} onChange={setConfig} placeholder={'{ "mcpServers": { … } }'} lines={[6, 16]} mono /> : null}
          {mode !== 'config' ? <Field value={name} onChange={setName} placeholder="Name" /> : null}
          {mode !== 'config' ? <Field value={target} onChange={setTarget} placeholder={mode === 'url' ? 'https://example.com/mcp' : 'npx -y @scope/server'} mono /> : null}
        </CardSection>
        {mode === 'url' ? (
          <CardSection title="Headers" footer={onHost('Optional, one Name: value per line. Leave empty if the service has you sign in. Saved on {name} only.')}>
            <Field value={envText} onChange={setEnvText} placeholder="Authorization: Bearer …" lines={[2, 6]} mono />
          </CardSection>
        ) : mode === 'command' ? (
          <CardSection title="Environment" footer={onHost('One KEY=value per line. Saved on {name} only.')}>
            <Field value={envText} onChange={setEnvText} placeholder="API_KEY=…" lines={[2, 6]} mono />
          </CardSection>
        ) : null}
        {error ? <CardSection><ErrorText text={error} /></CardSection> : null}
      </CardForm>
    </SheetPage>
  )
}

export function NewSkillSheet() {
  const tr = useTr()
  const store = useStore()
  const plugins = usePlugins(store)
  const dismiss = useDismiss()
  const [name, setName] = useState('')
  const [summary, setSummary] = useState('')
  const [instructions, setInstructions] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const save = async () => {
    setSaving(true)
    try {
      await plugins.addSkill(name, summary, instructions)
      dismiss()
    } catch (e) {
      setError(errorText(e))
    }
    setSaving(false)
  }

  return (
    <SheetPage>
      <ModalHeader
        icon="square.and.pencil"
        title={tr(`New skill`)}
        trailing={saving ? <Spinner /> : <IconButton title="Save" icon="checkmark" disabled={!name.trim() || !instructions.trim()} onClick={() => void save()} />}
      />
      <CardForm>
        <CardSection footer="The bot sees the name and when to use it, and reads the instructions only when a task fits.">
          <Field value={name} onChange={setName} placeholder="Name" />
          <Field value={summary} onChange={setSummary} placeholder="When to use it" lines={[2, 4]} />
        </CardSection>
        <CardSection title="Instructions">
          <Field value={instructions} onChange={setInstructions} placeholder="Step by step, in plain words…" lines={[6, 20]} />
        </CardSection>
        {error ? <CardSection><ErrorText text={error} /></CardSection> : null}
      </CardForm>
    </SheetPage>
  )
}
