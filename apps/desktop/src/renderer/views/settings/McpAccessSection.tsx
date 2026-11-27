import { useCallback, useEffect, useState } from 'react'
import { CardSection, ChoicePicker, IconButton, ValueRow } from '../../components/Controls'
import { Dialog, DialogField } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { activeClients, clientDetail, mcpCommand, mcpLevels, readMcpAccess, type McpAccess, type McpClient, type McpLevel } from '../../lib/mcp-access'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import type { BotStore } from '../../store/bot-store'
import { errorText, liveClient } from './parts'

/** The outside apps (coding agents, assistants) that may drive this computer's threads over MCP, and how far. */
export function McpAccessSection({ store }: { store: BotStore }) {
  const tr = useTr()
  useModel(store)
  // `null` while loading or on a host from before MCP access, which shows nothing.
  const [access, setAccess] = useState<McpAccess | null>(null)
  const [naming, setNaming] = useState<string | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const [confirmRevoke, setConfirmRevoke] = useState<McpClient | null>(null)
  const online = store.connection.kind === 'online'

  const load = useCallback(async () => {
    const client = liveClient(store)
    if (!client) return
    try {
      setAccess(readMcpAccess(await client.call('mcpClients')))
    } catch {
      setAccess(null)
    }
  }, [store])
  useEffect(() => void load(), [load, online])

  const run = async (method: string, body: Record<string, unknown>) => {
    try {
      const answer = await (await store.ready()).call<{ token?: string }>(method, body)
      if (answer?.token) setToken(answer.token)
    } catch (e) {
      store.setError(errorText(e))
    }
    await load()
  }

  if (!access) return null
  const levelOptions = mcpLevels.map((l) => ({ id: l.id, label: tr(l.label) }))
  const clients = activeClients(access)
  const create = () => {
    const label = (naming ?? '').trim()
    if (label) void run('mcpClientCreate', { label, access: 'ask', bots: ['*'], plainThreads: true, scopes: ['threads'] })
  }

  return (
    <>
      <CardSection
        title={tr('MCP access')}
        accessory={<IconButton title="Give an app access" icon="plus" onClick={() => setNaming('')} />}
        footer={tr('Approvals are never answered through MCP: every request still comes to your devices.')}
      >
        {[
          <ValueRow key="max" label="Most an app may do" detail="An app given more acts as if given this.">
            <ChoicePicker selection={access.levels.max} options={levelOptions} onChange={(max) => void run('setMcpAccess', { max })} />
          </ValueRow>,
          <ValueRow key="local" label="Sessions on this computer" detail="Coding agents here without a token. Any program of yours can act as one.">
            <ChoicePicker selection={access.levels.local} options={levelOptions} onChange={(local) => void run('setMcpAccess', { local })} />
          </ValueRow>,
          <ValueRow key="cloud" label="Cloud assistants" detail="ChatGPT or Claude through your Codync account. Codync's cloud can read these calls.">
            <ChoicePicker selection={access.levels.cloud} options={levelOptions} onChange={(cloud) => void run('setMcpAccess', { cloud })} />
          </ValueRow>,
          ...clients.map((client) => (
            <HStack key={client.id} gap={8} className="settings-row settings-appear">
              <VStack grow className="settings-stack">
                <span>{client.label}</span>
                <Text size="caption" tone="tertiary">{clientDetail(client, tr)}</Text>
              </VStack>
              <ChoicePicker selection={client.access} options={levelOptions} onChange={(level: McpLevel) => void run('mcpClientUpdate', { id: client.id, access: level })} />
              <IconButton title="Renew for 30 days" icon="arrow.clockwise" onClick={() => void run('mcpClientUpdate', { id: client.id, days: 30 })} />
              <IconButton title="Revoke access" icon="xmark.circle" onClick={() => setConfirmRevoke(client)} />
            </HStack>
          )),
        ]}
      </CardSection>
      <Dialog
        open={naming !== null}
        title={tr('Give an app access')}
        message={tr('It starts at Ask for every request on any bot, for 30 days. You can change it after.')}
        actions={[{ title: 'Create', disabled: !(naming ?? '').trim(), action: create }]}
        onClose={() => setNaming(null)}
      >
        <DialogField value={naming ?? ''} onChange={setNaming} onSubmit={() => { create(); setNaming(null) }} placeholder={tr('Name')} label={tr('Name')} />
      </Dialog>
      <Dialog
        open={token !== null}
        title={tr('Copy its token now')}
        message={tr("It isn't shown again. Anyone with it can act as this app: keep it out of chats and repositories.")}
        actions={token ? [{ title: 'Copy', action: () => void navigator.clipboard.writeText(mcpCommand(token)) }] : []}
        cancel="Done"
        onClose={() => setToken(null)}
      >
        <Text as="div" size="caption" mono className="settings-token">{token ? mcpCommand(token) : ''}</Text>
      </Dialog>
      <Dialog
        open={confirmRevoke !== null}
        title={tr('Revoke {name}?').replaceAll('{name}', confirmRevoke?.label ?? '')}
        message={tr('Its token stops working right away.')}
        actions={confirmRevoke ? [{ title: 'Revoke access', destructive: true, action: () => void run('mcpClientRevoke', { id: confirmRevoke.id }) }] : []}
        onClose={() => setConfirmRevoke(null)}
      />
    </>
  )
}
