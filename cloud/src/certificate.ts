// Device certificates: the cloud vouches that a device key belongs to an account, so a computer can
// check a pairing phone's account offline (docs/plans/require-sign-in.md, phase 2).

import { b64url, utf8 } from "./auth";

/** Long enough to pair at a computer with no network for a while; the phone fetches a new one before it runs out. */
export const CERT_TTL_MS = 7 * 24 * 60 * 60_000;
export const CERT_DOMAIN = "codync/device-cert/v1\n";

export interface CertificatePayload {
  v: 1;
  deviceKey: string;
  userId: string;
  email: string | null;
  exp: number;
}

let signer: Promise<CryptoKey> | undefined;
let signerSource: string | undefined;

/** Signs `payload` with `jwk` (an Ed25519 private key as JWK JSON, the `DEVICE_CERT_KEY` secret). */
export async function certify(jwk: string, payload: CertificatePayload): Promise<{ payload: string; sig: string }> {
  if (signerSource !== jwk) {
    signerSource = jwk;
    signer = crypto.subtle.importKey("jwk", JSON.parse(jwk), { name: "Ed25519" }, false, ["sign"]);
  }
  const text = JSON.stringify(payload);
  const sig = await crypto.subtle.sign({ name: "Ed25519" }, await signer!, utf8(CERT_DOMAIN + text));
  return { payload: text, sig: b64url(new Uint8Array(sig)) };
}
