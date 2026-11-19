// Names the computer straight through the host's API (HOST_PORT and HOST_TOKEN, passed with `-e`), the way another
// device does, for `computer-name.yaml`. `NAME` is the name to set (empty: its hostname, which the first call keeps in
// `output.hostname`); `EXPECT` instead checks the name the host has now ("hostname" for its hostname).
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (typeof EXPECT !== 'undefined') {
  const want = EXPECT === 'hostname' ? output.hostname : EXPECT
  const name = api('hello', {}).name
  if (name !== want) throw new Error(`the host's name is ${name}, not ${want}`)
} else {
  const name = api('setComputerName', { name: NAME }).name
  if (output.hostname === undefined) output.hostname = name
}
