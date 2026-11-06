/** A host-signed ACL. The bytes of `d` are verified before this parses them. */

import { fromB64url } from './codec.ts'

export interface Acl {
  v: 1
  computerId: string
  ver: number
  devices: { dk: string; grant: string | null }[]
  offers: { id: string; exp: number }[]
}

/** Parses and shape-checks a signed ACL's JSON. `null` when malformed. */
export function parseAcl(bytes: Uint8Array): Acl | null {
  let acl: unknown
  try {
    acl = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: false }).decode(bytes))
  } catch {
    return null
  }
  if (!acl || typeof acl !== 'object') return null
  const a = acl as Partial<Acl>
  const devices = a.devices
  const offers = a.offers
  const ok =
    a.v === 1 &&
    typeof a.computerId === 'string' &&
    Number.isSafeInteger(a.ver) &&
    (a.ver ?? 0) >= 1 &&
    Array.isArray(devices) &&
    devices.every(
      (d) => !!d && fromB64url(d.dk, 32) !== null && (d.grant === null || (typeof d.grant === 'string' && d.grant.length <= 64)),
    ) &&
    Array.isArray(offers) &&
    offers.every((o) => !!o && fromB64url(o.id, 16) !== null && Number.isSafeInteger(o.exp))
  return ok ? (a as Acl) : null
}
