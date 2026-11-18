// Helps `login-profiles.yaml`, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with `-e`).
// Without `STEP` it adds an account login profile (signed out: nothing here signs in) to an installed agent that can
// have them and makes a bot on it, and names both that login profile and the agent's default one, so the flow finds
// them by name. Phones in one run share that host and the default login profile's name is the agent's, so each takes
// an agent of its own: the iPhone the first, Android the last, which sits the flow out when the computer has only
// one. `renamed` is the name the flow gives the default login profile with its pencil; `keyName` the made-up variable
// it types into an API key login profile. `STEP` checks what the phone did reached the host; `CLEANUP`, when the flow
// completes, deletes what is left and gives the default login profile its name back.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}
const made = output.profiles
// The agent's login profiles on the host, and the API key ones holding the flow's made-up variable.
const profiles = () => api('hello', {}).backends.find((b) => b.id === made.agent).loginProfiles
const keyed = () => profiles().filter((p) => p.kind === 'apiKey' && p.vars.indexOf(made.keyName) >= 0)
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
    try { api('removeLoginProfile', { loginProfile: made.account }) } catch (e) {} // already removed on the phone
    keyed().forEach((p) => { try { api('removeLoginProfile', { loginProfile: p.id }) } catch (e) {} })
    try { api('renameLoginProfile', { backend: made.agent, name: made.ownLabel || '' }) } catch (e) {}
  }
} else if (typeof STEP === 'undefined') {
  // One whose account login profiles sign in in a terminal: nothing here may open a sign-in in a browser on the computer.
  const agents = api('hello', {}).backends.filter((b) => b.installed && b.loginProfileSignIn === 'terminal')
  const onIphone = maestro.platform === 'ios'
  const agent = onIphone ? agents[0] : agents.length > 1 ? agents[agents.length - 1] : undefined
  if (agent) {
    const account = api('addLoginProfile', { backend: agent.id, kind: 'account' }).loginProfile
    if (account.name !== null || account.signedIn) throw new Error(`a new login profile reads ${JSON.stringify(account)}`)
    const tag = Math.random().toString(36).slice(2, 6)
    const name = `Login check ${tag}`
    const [work, home, renamed, keyName] = [`Work ${tag}`, `Home ${tag}`, `Den ${tag}`, `MAESTRO_MADE_UP_${tag.toUpperCase()}`]
    api('renameLoginProfile', { backend: agent.id, loginProfile: account.id, name: work })
    api('renameLoginProfile', { backend: agent.id, name: home })
    const bot = api('createBot', { name, backend: agent.id, loginProfile: account.id, connectors: [], skills: [], notify: false }).bot
    if (bot.loginProfile !== account.id) throw new Error(`the bot runs on ${bot.loginProfile}, not ${account.id}`)
    const ownLabel = (agent.defaultLoginProfile && agent.defaultLoginProfile.label) || null
    // The key form lists the agent's usual variables first. Their value fields read "Value" like the new row's; on
    // the iPhone their filled name fields still answer to "Variable name" too.
    const usual = (agent.loginKeys || []).length
    const [nameIndex, valueIndex] = [onIphone ? usual : 0, usual]
    output.profiles = { name, work, home, renamed, keyName, nameIndex, valueIndex, ownLabel, bot: bot.id, account: account.id, agent: agent.id }
  } else {
    output.profiles = null
  }
} else if (STEP === 'refused') {
  let refused = false
  try { api('removeLoginProfile', { loginProfile: made.account }) } catch (e) { refused = true }
  if (!refused) throw new Error('a login profile a bot still uses was removed')
} else if (STEP === 'moved') {
  eventually('the bot is still on the login profile', () => api('updateBot', { id: made.bot }).bot.loginProfile == null)
} else if (STEP === 'renamed') {
  const label = () => api('hello', {}).backends.find((b) => b.id === made.agent).defaultLoginProfile.label
  eventually('the default login profile kept its old name', () => label() === made.renamed)
} else if (STEP === 'removed') {
  eventually('the login profile is still on the computer', () => !profiles().some((p) => p.id === made.account))
} else if (STEP === 'keyed') {
  eventually('the API key login profile has no such variable', () => keyed().some((p) => p.signedIn && p.vars.length === 1))
} else if (STEP === 'unkeyed') {
  eventually('the API key login profile is still on the computer', () => keyed().length === 0)
}
