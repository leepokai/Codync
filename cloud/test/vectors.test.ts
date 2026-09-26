// docs/reference/fixtures/remote-relay-vectors.json, field by field: the TS reference implementation and the Worker's own code.

import { describe, expect, it } from "vitest";
import V from "../../docs/reference/fixtures/remote-relay-vectors.json";
import { claimCanonical as workerClaimCanonical } from "../src/api";
import { computerIdFor, fromB64url, sigCanonical, verifyEd25519, verifySig } from "../src/auth";
import { parseAcl } from "../src/relay";
import * as ref from "./ref";

const u = ref.unb64url;
const hostSign = ref.signKey(u(V.keys.hostSignSeed));
const hostBox = ref.boxKey(u(V.keys.hostBoxPriv));
const device = ref.signKey(u(V.keys.deviceSignSeed));
const cid = ref.cidRaw(hostSign.pub);

describe("keys", () => {
  it("derives the public keys and computerId", async () => {
    expect(ref.b64url(hostSign.pub)).toBe(V.keys.hostSignPub);
    expect(ref.b64url(hostBox.pub)).toBe(V.keys.hostBoxPub);
    expect(ref.b64url(device.pub)).toBe(V.keys.deviceSignPub);
    expect(ref.computerId(hostSign.pub)).toBe(V.keys.computerId);
    expect(await computerIdFor(hostSign.pub)).toBe(V.keys.computerId);
  });
});

describe("handshake", () => {
  const h = V.handshake;
  const ekD = ref.boxKey(u(h.deviceEphPriv));
  const ekH = ref.boxKey(u(h.hostEphPriv));
  const n = u(h.n);

  it("matches every intermediate value", () => {
    expect(ref.b64url(ekD.pub)).toBe(h.deviceEphPub);
    expect(ref.b64url(ekH.pub)).toBe(h.hostEphPub);
    const hs1 = ref.hs1Input(cid, device.pub, ekD.pub, n);
    expect(ref.b64url(hs1)).toBe(h.hs1SignInput);
    expect(ref.b64url(ref.sign(device, hs1))).toBe(h.hs1Sig);
    const th = ref.transcriptHash(cid, device.pub, ekD.pub, n, ekH.pub);
    expect(ref.b64url(th)).toBe(h.transcriptHash);
    expect(ref.b64url(ref.sign(hostSign, th))).toBe(h.hs2Sig);
    const keys = ref.channelKeys(ekD.priv, ekH.pub, th);
    expect(ref.b64url(keys.ss)).toBe(h.sharedSecret);
    expect(ref.b64url(keys.prk)).toBe(h.prk);
    expect(ref.b64url(keys.d2h)).toBe(h.kD2H);
    expect(ref.b64url(keys.h2d)).toBe(h.kH2D);
  });

  it("runs device and host sides against each other", () => {
    const d = ref.deviceHello(device, hostSign.pub, { eph: ekD, n });
    expect(d.hello.sig).toBe(h.hs1Sig);
    const hostSide = ref.hostWelcome(hostSign, d.hello as never, ekH);
    expect(hostSide.welcome.sig).toBe(h.hs2Sig);
    const keys = d.finish(hostSide.welcome);
    expect(ref.b64url(keys.d2h)).toBe(h.kD2H);
    expect(ref.b64url(hostSide.keys.h2d)).toBe(h.kH2D);
  });

  it("rejects a welcome signed by another key", () => {
    const d = ref.deviceHello(device, hostSign.pub, { eph: ekD, n });
    const th = ref.transcriptHash(cid, device.pub, ekD.pub, n, ekH.pub);
    expect(() => d.finish({ ek: h.hostEphPub, sig: ref.b64url(ref.sign(ref.signKey(), th)) })).toThrow();
    expect(() => d.finish({ ek: h.hostEphPub, sig: h.hs2Sig })).not.toThrow();
  });

  it("rejects an all-zero shared secret", () => {
    expect(() => ref.channelKeys(ekD.priv, new Uint8Array(32), new Uint8Array(32))).toThrow();
  });
});

describe("frames", () => {
  const keys = { d2h: u(V.handshake.kD2H), h2d: u(V.handshake.kH2D) };
  for (const f of V.frames) {
    it(`${f.dir} c=${f.c}`, () => {
      const key = f.dir === "d2h" ? keys.d2h : keys.h2d;
      const pt = ref.concat(new Uint8Array([f.final ? 0 : 1]), ref.enc.encode(f.message));
      expect(ref.b64url(pt)).toBe(f.plaintext);
      expect(ref.sealFrame(key, f.c, f.final, ref.enc.encode(f.message))).toBe(f.d);
      const opened = ref.openFrame(key, f.c, f.d);
      expect(opened.final).toBe(f.final);
      expect(ref.dec.decode(opened.chunk)).toBe(f.message);
    });
  }

  it("refuses the wrong counter or direction", () => {
    const f = V.frames[0]!;
    expect(() => ref.openFrame(keys.d2h, 1, f.d)).toThrow();
    expect(() => ref.openFrame(keys.h2d, 0, f.d)).toThrow();
  });

  it("chunks at 256 KiB and reassembles", () => {
    const device = new ref.FrameCodec(keys.d2h, keys.h2d);
    const host = new ref.FrameCodec(keys.h2d, keys.d2h);
    const big = { id: 9, m: "send", b: { text: "x".repeat(600 * 1024) } };
    const frames = device.seal(big);
    expect(frames.length).toBe(3);
    const out = frames.map((f) => host.open(f));
    expect(out.slice(0, 2)).toEqual([undefined, undefined]);
    expect(out[2]).toEqual(big);
    // A skipped counter closes the channel (replay/reorder protection).
    expect(() => host.open({ c: 5, d: device.seal({ id: 1 })[0]!.d })).toThrow();
  });
});

describe("mailbox", () => {
  const m = V.mailbox;
  it("seals the vector blob", () => {
    const eph = ref.boxKey(u(m.ephPriv));
    expect(ref.b64url(eph.pub)).toBe(m.ephPub);
    const blob = ref.sealMailbox(device, hostSign.pub, hostBox.pub, m.clientNonce, m.plaintext, eph);
    expect(ref.b64url(blob)).toBe(m.blob);
    expect(ref.b64url(blob.subarray(32, 96))).toBe(m.sig);
    expect(ref.b64url(blob.subarray(96))).toBe(m.ciphertext);
    const key = ref.mailboxKey(u(m.sharedSecret), cid, device.pub, eph.pub);
    expect(ref.b64url(key)).toBe(m.key);
  });

  it("opens with the host box key and checks the clientNonce binding", () => {
    expect(ref.openMailbox(hostSign.pub, hostBox, device.pub, m.clientNonce, u(m.blob))).toBe(m.plaintext);
    expect(() => ref.openMailbox(hostSign.pub, hostBox, device.pub, "other", u(m.blob))).toThrow();
  });

  it("uses a fresh ephemeral key for every seal", () => {
    const a = ref.sealMailbox(device, hostSign.pub, hostBox.pub, m.clientNonce, m.plaintext);
    const b = ref.sealMailbox(device, hostSign.pub, hostBox.pub, m.clientNonce, m.plaintext);
    expect(ref.b64url(a.subarray(0, 32))).not.toBe(ref.b64url(b.subarray(0, 32)));
  });
});

describe("SAS, offer, claim, push", () => {
  it("sas", () => {
    const nD = u(V.sas.deviceNonce);
    const nH = u(V.sas.hostNonce);
    expect(ref.b64url(ref.sasCommit(device.pub, nD))).toBe(V.sas.commit);
    expect(ref.sasCode(hostSign.pub, device.pub, nD, nH)).toBe(V.sas.code);
  });

  it("offer id", () => {
    expect(ref.offerId(u(V.pairing.code))).toBe(V.pairing.offerId);
  });

  it("claim", async () => {
    const c = V.claim;
    const canonical = ref.claimCanonical(c.claimId, c.nonce, c.userId, c.computerId, c.boxKey);
    expect(canonical).toBe(c.canonical);
    expect(workerClaimCanonical(c.claimId, c.nonce, c.userId, c.computerId, c.boxKey)).toBe(c.canonical);
    expect(ref.b64url(ref.sign(hostSign, ref.enc.encode(canonical)))).toBe(c.sig);
    expect(await verifyEd25519(hostSign.pub, u(c.sig), ref.enc.encode(canonical))).toBe(true);
  });

  it("push", () => {
    const p = V.push;
    const push = ref.boxKey(u(p.pushPriv));
    expect(ref.b64url(push.pub)).toBe(p.pushPub);
    const eph = ref.boxKey(u(p.ephPriv));
    expect(ref.b64url(eph.pub)).toBe(p.ephPub);
    expect(ref.b64url(ref.pushKey(u(p.sharedSecret), cid, push.pub, eph.pub))).toBe(p.key);
    expect(ref.sealPush(hostSign.pub, push.pub, p.plaintext, eph)).toBe(p.sealed);
    expect(ref.openPush(hostSign.pub, push, p.sealed)).toBe(p.plaintext);
  });
});

describe("Codync-Sig", () => {
  const r = V.requestSig;
  const req = (headers: Record<string, string>, authority = r.authority, path = r.pathAndQuery) =>
    new Request(`https://${authority}${path}`, { method: r.method, headers });

  it("builds the canonical string and header", async () => {
    expect(ref.b64url(await import("@noble/hashes/sha2.js").then((m) => m.sha256(new Uint8Array())))).toBe(r.bodySha256);
    expect(ref.sigCanonical(r.method, r.authority, r.pathAndQuery, r.ts, r.nonce, new Uint8Array())).toBe(r.canonical);
    expect(await sigCanonical(r.method, r.authority, r.pathAndQuery, r.ts, r.nonce, new Uint8Array())).toBe(r.canonical);
    const header = ref.signRequest(device, { method: r.method, authority: r.authority, pathAndQuery: r.pathAndQuery, ts: r.ts, nonce: r.nonce });
    expect(`Codync-Sig: ${header}`).toBe(r.header);
  });

  it("verifies in the Worker within the time window only", async () => {
    const value = r.header.slice("Codync-Sig: ".length);
    expect(await verifySig(req({ "Codync-Sig": value }), new Uint8Array(), r.ts)).toEqual({
      kid: V.keys.deviceSignPub,
      ts: r.ts,
      nonce: r.nonce,
    });
    expect(await verifySig(req({ "Codync-Sig": value }), new Uint8Array(), r.ts + 300_000)).not.toBeNull();
    expect(await verifySig(req({ "Codync-Sig": value }), new Uint8Array(), r.ts + 300_001)).toBeNull();
    expect(await verifySig(req({ "Codync-Sig": value }), new Uint8Array(), r.ts - 300_001)).toBeNull();
  });

  it("binds the authority, path and body", async () => {
    const value = r.header.slice("Codync-Sig: ".length);
    // Upper-case Host is the same authority; a different host is not.
    expect(await verifySig(req({ "Codync-Sig": value }, r.authority.toUpperCase()), new Uint8Array(), r.ts)).not.toBeNull();
    expect(await verifySig(req({ "Codync-Sig": value }, "codync-cloud.example.workers.dev"), new Uint8Array(), r.ts)).toBeNull();
    expect(await verifySig(req({ "Codync-Sig": value }, r.authority, `${r.pathAndQuery}&pair=x`), new Uint8Array(), r.ts)).toBeNull();
    expect(await verifySig(req({ "Codync-Sig": value }), new Uint8Array([1]), r.ts)).toBeNull();
    expect(await verifySig(req({ "Codync-Sig": value.replace("v=1", "v=2") }), new Uint8Array(), r.ts)).toBeNull();
    expect(await verifySig(req({}), new Uint8Array(), r.ts)).toBeNull();
  });
});

describe("ACL", () => {
  it("verifies the vector ACL signature and parses it", async () => {
    const a = V.acl;
    expect(ref.b64url(ref.enc.encode(a.json))).toBe(a.d);
    expect(ref.aclMessage(hostSign, a.json)).toEqual({ t: "acl", d: a.d, sig: a.sig });
    expect(await verifyEd25519(hostSign.pub, u(a.sig), u(a.d))).toBe(true);
    expect(parseAcl(u(a.d))).toEqual(JSON.parse(a.json));
    expect(parseAcl(ref.enc.encode('{"v":2}'))).toBeNull();
  });
});

describe("base64url", () => {
  it("is strict", () => {
    expect(fromB64url("AA", 1)).toEqual(new Uint8Array([0]));
    expect(fromB64url("AB", 1)).toBeNull(); // non-canonical trailing bits
    expect(fromB64url("AA==")).toBeNull();
    expect(fromB64url("A+")).toBeNull();
    expect(fromB64url("AAA", 1)).toBeNull();
  });
});
