// HMAC-SHA256 in hex: the signed session cookie and GitHub's webhook signature.

const encoder = new TextEncoder();

const key = (secret: string) =>
  crypto.subtle.importKey("raw", encoder.encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign", "verify"]);

export async function sign(secret: string, data: string): Promise<string> {
  const mac = await crypto.subtle.sign("HMAC", await key(secret), encoder.encode(data));
  return [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** Constant-time check of a hex signature. */
export async function verify(secret: string, data: string, signature: string): Promise<boolean> {
  const pairs = /^(?:[0-9a-f]{2})+$/.test(signature) ? signature.match(/../g) : null;
  if (!pairs) return false;
  const bytes = Uint8Array.from(pairs, (h) => parseInt(h, 16));
  return crypto.subtle.verify("HMAC", await key(secret), bytes, encoder.encode(data));
}
