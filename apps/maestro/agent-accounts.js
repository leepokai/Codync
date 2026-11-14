// Helps `agent-accounts.yaml`, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with `-e`).
// Without `STEP` it adds an account (signed out: nothing here signs in) to an installed agent that can have them
// (Claude Code or Codex) and makes a bot on it. Phones in one run share that host and every new account reads
// "New account", so each takes an agent of its own: the iPhone the first, Android the last, which sits the flow out
// when the computer has only one. `STEP` checks what the phone did reached the host; `CLEANUP`, when the flow
// completes, deletes what is left.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}
const made = output.accounts
// The apps save 600 ms after the last change, and a removal answers before the list reaches the script.
const eventually = (what, done) => {
  for (let tries = 0; tries < 10; tries++) {
    if (done()) return
    const until = Date.now() + 300
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 3 s
  }
  throw new Error(what)
}

if (typeof CLEANUP !== 'undefined') {
  if (made) {
    try { api('deleteBot', { botId: made.bot }) } catch (e) {}
    try { api('removeAgentAccount', { account: made.account }) } catch (e) {} // already removed on the phone
  }
} else if (typeof STEP === 'undefined') {
  const agents = api('hello', {}).backends.filter((b) => b.accounts && b.installed)
  const onIphone = maestro.platform === 'ios'
  const agent = onIphone ? agents[0] : agents.length > 1 ? agents[agents.length - 1] : undefined
  if (agent) {
    const account = api('addAgentAccount', { backend: agent.id }).account
    if (account.name !== null || account.signedIn) throw new Error(`a new account reads ${JSON.stringify(account)}`)
    const name = `Account check ${Math.random().toString(36).slice(2, 6)}`
    const bot = api('createBot', { name, backend: agent.id, account: account.id, connectors: [], skills: [], notify: false }).bot
    if (bot.account !== account.id) throw new Error(`the bot runs on ${bot.account}, not ${account.id}`)
    output.accounts = { name, bot: bot.id, account: account.id, agent: agent.id }
  } else {
    output.accounts = null
  }
} else if (STEP === 'refused') {
  let refused = false
  try { api('removeAgentAccount', { account: made.account }) } catch (e) { refused = true }
  if (!refused) throw new Error('an account a bot still uses was removed')
} else if (STEP === 'moved') {
  eventually('the bot is still on the account', () => api('updateBot', { id: made.bot }).bot.account == null)
} else if (STEP === 'removed') {
  const listed = () => api('hello', {}).backends.find((b) => b.id === made.agent).accounts.some((a) => a.id === made.account)
  eventually('the account is still on the computer', () => !listed())
}
