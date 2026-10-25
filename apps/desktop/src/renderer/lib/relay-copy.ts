/** What the interface calls the computer's relay. Official clouds stay "Cloudflare". */

const OFFICIAL = new Set(['api.codync.dev', 'dev-api.codync.dev'])

/** Host, with a non-default port. Null for the official Codync clouds or when there is no URL. */
export function relayHost(cloud: string | null | undefined): string | null {
  if (!cloud) return null
  let url: URL
  try {
    url = new URL(cloud)
  } catch {
    return null
  }
  const hostname = url.hostname.toLowerCase()
  if (OFFICIAL.has(hostname) && url.port === '') return null
  // `URL.hostname` already brackets an IPv6 address.
  return url.port ? `${hostname}:${url.port}` : hostname
}

export function connectedThrough(cloud: string | null | undefined): string {
  const host = relayHost(cloud)
  return host ? `Connected through Private relay (${host})` : 'Connected through Cloudflare'
}

/** The desktop "Reach from anywhere" caption while there is no error to show. */
export function cloudStatusLine(cloud: { enabled: boolean; url?: string | null; connected?: boolean | null }): string {
  if (!cloud.enabled) return 'Off: only Wi-Fi and Tailscale reach it, nothing goes through Cloudflare.'
  const host = relayHost(cloud.url)
  if (!host) return cloud.connected ? 'Reachable from anywhere through Cloudflare (encrypted).' : 'Connecting to Cloudflare…'
  return cloud.connected
    ? `Reachable from anywhere through Private relay (${host}) (encrypted).`
    : `Connecting to Private relay (${host})…`
}
