import { useEffect, useMemo, useState } from 'react'
import { Button, CardSection, ChoicePicker } from '../../components/Controls'
import { Dialog } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { allowedText, lastReceivedText, pairingInstruction, telegramNameOk, telegramStateText, telegramTokenOk, type TelegramBotStatus } from '../../../shared/channels'
import { useApp } from '../../store/context'
import { ChannelsStore } from '../../store/channels-store'
import { ErrorText, Field } from '../marketplace/pieces'

/** Telegram bots (add, pair, who may write, which bot answers, state) and the Slack connection of this computer's host. */
export function ChannelsSection() {
  const app = useApp()
  const local = app.local
  const model = useModel(useMemo(() => new ChannelsStore(async () => {
    if (!local) throw new Error('Start this computer’s host to set up channels.')
    return local.ready()
  }), [local]))
  const [removing, setRemoving] = useState<string | null>(null)
  const [disconnecting, setDisconnecting] = useState(false)
  useEffect(() => {
    model.resume(); void model.load()
    const refresh = setInterval(() => void model.load(), 15_000)
    return () => { clearInterval(refresh); model.dispose() }
  }, [model])
  const slack = model.slack
  return <CardSection title="Channels" footer="Talk to a bot from Telegram or Slack. Only people you pair or allow can write to it; tokens stay in this computer’s credential vault.">
    {model.telegram.map((bot) => <TelegramRow key={bot.name} bot={bot} model={model} onRemove={() => setRemoving(bot.name)} />)}
    {model.telegram.length === 0 ? <Text tone="secondary">No Telegram bots yet.</Text> : null}
    <AddTelegram model={model} bots={(local?.roster ?? []).filter((b) => b.kind !== 'group' && b.backend !== 'external').map((b) => ({ id: b.id, label: b.name }))} />
    {slack?.configured ? <VStack gap={4} className="settings-stack">
      <Text>Slack {slack.socketMode ? '· Socket Mode' : '· HTTP endpoint'}</Text>
      <Text size="caption" tone="tertiary">{slack.workspaceId} · {slack.bindings?.length ?? 0} channel{slack.bindings?.length === 1 ? '' : 's'} bound</Text>
      <Button kind="secondary" disabled={model.busy} onClick={() => setDisconnecting(true)}>Disconnect Slack</Button>
    </VStack> : <Text size="caption" tone="tertiary">Slack is connected with the host’s setSlackChannel call; see docs/guides/slack-manual-checklist.md.</Text>}
    {model.error && <ErrorText text={model.error} />}
    <Dialog open={removing !== null} title="Remove this Telegram bot?" message="Its token is deleted from this computer and the people paired to it can no longer write to your bot." actions={[{ title: 'Remove', action: () => { const name = removing; setRemoving(null); if (name) void model.removeTelegram(name) } }]} onClose={() => setRemoving(null)} />
    <Dialog open={disconnecting} title="Disconnect Slack?" message="Slack stops reaching your bots and queued prompts are cancelled." actions={[{ title: 'Disconnect', action: () => { setDisconnecting(false); void model.disconnectSlack() } }]} onClose={() => setDisconnecting(false)} />
  </CardSection>
}

function TelegramRow({ bot, model, onRemove }: { bot: TelegramBotStatus; model: ChannelsStore; onRemove: () => void }) {
  const code = model.pairing?.name === bot.name ? model.pairing.code : null
  return <VStack gap={6} className="settings-stack">
    <Text>{bot.name}{bot.username ? ` · @${bot.username}` : ''} → {bot.botName}</Text>
    <Text size="caption" tone={bot.state === 'ok' ? 'secondary' : 'tertiary'}>{telegramStateText(bot.state)} · {lastReceivedText(bot.lastReceivedAt, Date.now())} · {allowedText(bot.allowed.length)}</Text>
    {bot.allowed.map((id) => <div key={id} className="settings-row">
      <Text grow>Telegram user {id}</Text>
      <Button kind="secondary" disabled={model.busy} onClick={() => void model.setAllowed(bot.name, id, false)}>Remove</Button>
    </div>)}
    {code ? <Text role="status">{pairingInstruction(code, bot.username)} <b>{code}</b></Text> : null}
    <div className="settings-row">
      <Button kind="secondary" disabled={model.busy} onClick={() => void model.pair(bot.name)}>Pair someone</Button>
      <Button kind="secondary" disabled={model.busy} onClick={onRemove}>Remove bot</Button>
    </div>
  </VStack>
}

function AddTelegram({ model, bots }: { model: ChannelsStore; bots: { id: string; label: string }[] }) {
  const [name, setName] = useState('')
  const [token, setToken] = useState('')
  const [botId, setBotId] = useState('')
  const chosen = botId || bots[0]?.id || ''
  const ready = telegramNameOk(name) && telegramTokenOk(token.trim()) && chosen !== ''
  const add = async () => { if (await model.addTelegram(name, token.trim(), chosen)) { setName(''); setToken('') } }
  return <VStack gap={6} className="settings-stack">
    <Field label="Telegram bot name" value={name} onChange={setName} placeholder="A short name: home, work…" />
    <Field label="Telegram bot token" secret value={token} onChange={setToken} placeholder="The token @BotFather gave you" />
    <ChoicePicker selection={chosen} options={bots.map((b) => ({ id: b.id, label: b.label }))} onChange={setBotId} />
    <Button disabled={model.busy || !ready} onClick={() => void add()}>{model.busy ? 'Adding…' : 'Add Telegram bot'}</Button>
  </VStack>
}
