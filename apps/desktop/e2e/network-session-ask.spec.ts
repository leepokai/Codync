import { test, expect } from '@playwright/test'
import { writeFileSync } from 'node:fs'
import { fixtureCommand } from './fixture-command.ts'
import { api, attach } from './sandbox.ts'

// A scripted agent: \`ASK <address> <message>\` lists the bots it can message (coding agents' sessions only) and messages
// <address> through its team tools, replying with both results; a turn of replies says "Relayed: " and what it got.
const askAgent = `import json,subprocess,sys
servers=[]
def send(m): sys.stdout.write(json.dumps(m)+"\\n"); sys.stdout.flush()
def say(t): send({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":t}}}})
def team(calls):
    s=next(s for s in servers if s["name"]=="team")
    with subprocess.Popen([s["command"],*s["args"]],stdin=subprocess.PIPE,stdout=subprocess.PIPE,encoding="utf-8") as p:
        def rpc(i,m,a):
            p.stdin.write(json.dumps({"jsonrpc":"2.0","id":i,"method":m,"params":a})+"\\n"); p.stdin.flush()
            return json.loads(p.stdout.readline())
        rpc(0,"initialize",{"protocolVersion":"2025-06-18"})
        out=[rpc(i+1,"tools/call",{"name":n,"arguments":a})["result"]["content"][0]["text"] for i,(n,a) in enumerate(calls)]
        p.stdin.close()
        return out
for line in sys.stdin:
    m=json.loads(line); meth=m.get("method")
    if meth=="initialize": send({"jsonrpc":"2.0","id":m["id"],"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":False}}})
    elif meth=="session/new":
        servers=m["params"]["mcpServers"]; send({"jsonrpc":"2.0","id":m["id"],"result":{"sessionId":"s1"}})
    elif meth=="session/prompt":
        text=m["params"]["prompt"][0]["text"]; last=next((l for l in text.splitlines() if l.startswith("ASK ")),"")
        if "[Codync: these are bot replies" in text: say("Relayed: "+text)
        elif last.startswith("ASK "):
            _,to,msg=last.split(" ",2)
            listed,sent=team([("list_bots",{"kind":"codex","query":to}),("message_bot",{"botId":to,"message":msg})])
            say("Listed: "+listed+"\\nSent: "+sent)
        else: say("ok")
        send({"jsonrpc":"2.0","id":m["id"],"result":{"stopReason":"end_turn"}})
`
type Entry = { kind: string; data: Record<string, unknown> }
const reply = (entries: Entry[], prefix: string) =>
  entries.map((e) => String(e.data.text ?? '')).findLast((t) => t.startsWith(prefix)) ?? ''

test('a bot messages a coding agent session here by its address and gets its answer in its chat', async () => {
  const { browser, page } = await attach()
  const name = `Session ask e2e ${Date.now()}`
  const script = test.info().outputPath('ask_agent.py')
  writeFileSync(script, askAgent)
  // A Codex session on this computer, as \`codync-host mcp network\` would call for it.
  const agent = { kind: 'codex', session: `e2e-${Date.now()}`, process: `e2e-${process.pid}`, cwd: test.info().outputDir }
  const call = <T>(name: string, args: object) => api<T>(page, 'agentCall', { agent, name, arguments: args })
  let botId: string | undefined
  try {
    const { address } = await call<{ address: string }>('whoami', {})
    expect(address).toBeTruthy()
    botId = (await api<{ bot: { id: string } }>(page, 'createBot', {
      name, backend: 'custom', command: fixtureCommand(script), connectors: [], skills: [], notify: false,
    })).bot.id
    const history = async () => (await api<{ entries: Entry[] }>(page, 'history', { botId, limit: 50 })).entries

    // The bot finds the session among those it can message and messages it: the letter waits in the session's
    // inbox under the request's id.
    await api(page, 'send', { botId, text: `ASK ${address} is the API done?` })
    await expect.poll(async () => reply(await history(), 'Listed:'), { timeout: 20_000 }).toContain('requestId')
    const said = reply(await history(), 'Listed:')
    expect(said).toContain(`"id":"${address}"`)
    const request = /"requestId":"([^"]+)"/.exec(said)![1]!
    const inbox = await call<{ letters: string }>('check_inbox', {})
    expect(inbox.letters).toContain(`id="${request}"`)
    expect(inbox.letters).toContain('is the API done?')

    // The session answers it: the bot's chat shows the answer, as a reply turn and on the request's notice.
    const answered = await call<{ status: string }>('message_bot', { botId, message: 'yes, merged', replyTo: request })
    expect(answered.status).toBe('delivered')
    await page.locator('.roster-row', { hasText: name }).click()
    await expect(page.locator('.message.agent', { hasText: 'Relayed:' }).filter({ hasText: 'yes, merged' })).toBeVisible({ timeout: 20_000 })
    const notice = (await history()).find((e) => e.data.delegationId === request)
    expect(notice?.data.status).toBe('completed')
    expect(JSON.stringify(notice?.data)).toContain('yes, merged')
  } finally {
    if (botId) await api(page, 'deleteBot', { botId }).catch(() => {})
    await browser.close()
  }
})
