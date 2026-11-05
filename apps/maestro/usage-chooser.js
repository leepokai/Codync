// Starts `usage-chooser.yaml` with one agent off on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed
// with `-e`), and checks what the switch did there: `EXPECT` is the state the host must hold. Phones in one run share
// that host, so each takes an agent of its own, from the end of the list: the ones a computer rarely has a login for.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}
const services = api('usage', {}).services
const service = services[services.length - (maestro.platform === 'ios' ? 1 : 2)]

if (typeof EXPECT !== 'undefined') {
  if (String(service.shown) !== EXPECT) throw new Error(`${service.id} shown is ${service.shown}, expected ${EXPECT}`)
} else {
  api('setUsageShown', { id: service.id, shown: false })
  output.agent = service.name
}
