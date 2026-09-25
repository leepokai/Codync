//! Wire cryptography of the phone ↔ host protocol (docs/remote-relay-spec.md §5, §6):
//! the signed-ephemeral handshake, frame sealing and chunking, mailbox opening,
//! push sealing, SAS, pairing offer ids and the canonical strings that get signed.
//!
//! Pure functions over bytes, no host state: `tests/e2e.rs` includes this file
//! as its device-side implementation, so it must only use external crates.
//! Every construction is checked against `docs/remote-relay-vectors.json`.

use anyhow::{Result, anyhow, bail, ensure};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

/// Largest plaintext chunk in one frame (§6.3).
pub const CHUNK: usize = 256 * 1024;
/// A channel is rekeyed (closed with `4011`) before either counter reaches this (§3.5).
pub const REKEY_AT: u64 = 1 << 32;

const FLAG_FINAL: u8 = 0x00;
const FLAG_MORE: u8 = 0x01;

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(s: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(s).map_err(|_| anyhow!("not base64url"))
}

/// Decodes base64url of exactly `N` bytes.
pub fn unb64_n<const N: usize>(s: &str) -> Result<[u8; N]> {
    unb64(s)?.try_into().map_err(|_| anyhow!("expected {N} bytes"))
}

pub fn random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).expect("the OS random source is always available on macOS and Linux");
    out
}

pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// Constant-time equality for secrets (pairing codes, tokens).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// `SHA-256(hostSignPub)[0..16]`: the raw computer id.
pub fn computer_id_raw(sign_pub: &[u8; 32]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(&sha256(&[sign_pub])[..16]);
    out
}

pub fn computer_id(sign_pub: &[u8; 32]) -> String {
    b64(&computer_id_raw(sign_pub))
}

pub fn verify(key: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> Result<()> {
    let key = VerifyingKey::from_bytes(key).map_err(|_| anyhow!("bad public key"))?;
    key.verify_strict(msg, &Signature::from_bytes(sig)).map_err(|_| anyhow!("bad signature"))
}

pub fn sign(key: &SigningKey, msg: &[u8]) -> [u8; 64] {
    use ed25519_dalek::Signer as _;
    key.sign(msg).to_bytes()
}

/// X25519 that refuses the all-zero result (a low-order peer key).
pub fn x25519(secret: &StaticSecret, public: &[u8; 32]) -> Result<[u8; 32]> {
    let ss = secret.diffie_hellman(&PublicKey::from(*public));
    ensure!(ss.was_contributory(), "low-order public key");
    Ok(ss.to_bytes())
}

pub fn x25519_pub(secret: &StaticSecret) -> [u8; 32] {
    PublicKey::from(secret).to_bytes()
}

fn hkdf_key(salt: &[u8], ikm: &[u8], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out = [0u8; 32];
    hk.expand(info, &mut out).expect("32 bytes is a valid HKDF-SHA256 output length");
    out
}

fn aead(key: &[u8; 32]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(key.into())
}

// MARK: handshake (§6.2)

/// What the device signs in `hello`.
pub fn hs1_input(cid_raw: &[u8; 16], dk: &[u8; 32], ek_d: &[u8; 32], n: &[u8; 32]) -> Vec<u8> {
    [b"codync/hs1/v1".as_slice(), cid_raw, dk, ek_d, n].concat()
}

/// `TH`: what the host signs in `welcome`, and the HKDF salt.
pub fn transcript_hash(cid_raw: &[u8; 16], dk: &[u8; 32], ek_d: &[u8; 32], n: &[u8; 32], ek_h: &[u8; 32]) -> [u8; 32] {
    sha256(&[b"codync/hs2/v1", cid_raw, dk, ek_d, n, ek_h])
}

/// Per-direction channel keys.
pub struct Keys {
    pub d2h: [u8; 32],
    pub h2d: [u8; 32],
}

pub fn channel_keys(shared: &[u8; 32], th: &[u8; 32]) -> Keys {
    let hk = Hkdf::<Sha256>::new(Some(th), shared);
    let mut keys = Keys { d2h: [0; 32], h2d: [0; 32] };
    hk.expand(b"codync/d2h/v1", &mut keys.d2h).expect("32 bytes is a valid HKDF-SHA256 output length");
    hk.expand(b"codync/h2d/v1", &mut keys.h2d).expect("32 bytes is a valid HKDF-SHA256 output length");
    keys
}

// MARK: frames (§6.3)

fn frame_nonce(c: u64) -> Nonce {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&c.to_be_bytes());
    n.into()
}

fn frame_aad(c: u64) -> Vec<u8> {
    [b"codync/frame/v1".as_slice(), &c.to_be_bytes()].concat()
}

pub fn seal_frame(key: &[u8; 32], c: u64, last: bool, chunk: &[u8]) -> Vec<u8> {
    let plaintext = [&[if last { FLAG_FINAL } else { FLAG_MORE }], chunk].concat();
    aead(key)
        .encrypt(&frame_nonce(c), Payload { msg: &plaintext, aad: &frame_aad(c) })
        .expect("ChaCha20-Poly1305 accepts any chunk up to 256 KiB")
}

/// Returns `(last, chunk)`.
pub fn open_frame(key: &[u8; 32], c: u64, d: &[u8]) -> Result<(bool, Vec<u8>)> {
    let mut plain = aead(key)
        .decrypt(&frame_nonce(c), Payload { msg: d, aad: &frame_aad(c) })
        .map_err(|_| anyhow!("frame doesn't decrypt"))?;
    ensure!(!plain.is_empty(), "empty frame");
    let flag = plain.remove(0);
    match flag {
        FLAG_FINAL => Ok((true, plain)),
        FLAG_MORE => Ok((false, plain)),
        _ => bail!("bad frame flag"),
    }
}

/// The sending half of a channel direction: counts frames from 0.
pub struct Sealer {
    key: [u8; 32],
    next: u64,
}

impl Sealer {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key, next: 0 }
    }

    /// One inner message as `f` frames (`{"t":"f","c","d"}`), or `None` once the counter must rekey.
    pub fn seal(&mut self, message: &[u8]) -> Option<Vec<serde_json::Value>> {
        let chunks: Vec<&[u8]> = if message.is_empty() { vec![&[]] } else { message.chunks(CHUNK).collect() };
        if self.next + chunks.len() as u64 > REKEY_AT {
            return None;
        }
        let total = chunks.len();
        Some(
            chunks
                .into_iter()
                .enumerate()
                .map(|(i, chunk)| {
                    let c = self.next;
                    self.next += 1;
                    serde_json::json!({"t": "f", "c": c, "d": b64(&seal_frame(&self.key, c, i + 1 == total, chunk))})
                })
                .collect(),
        )
    }
}

/// Why a received frame ends the channel.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Wrong counter, bad tag, malformed frame: close `4002`.
    Protocol,
    /// The reassembled message is over the limit: close `4013`.
    TooLarge,
    /// The counter reached 2^32: close `4011`.
    Rekey,
}

/// The receiving half: requires consecutive counters and reassembles chunks.
pub struct Opener {
    key: [u8; 32],
    next: u64,
    buf: Vec<u8>,
    limit: usize,
}

impl Opener {
    pub fn new(key: [u8; 32], limit: usize) -> Self {
        Self { key, next: 0, buf: vec![], limit }
    }

    /// Feeds one `f` frame; returns a whole inner message once its last chunk arrived.
    pub fn open(&mut self, frame: &serde_json::Value) -> Result<Option<Vec<u8>>, FrameError> {
        if self.next >= REKEY_AT {
            return Err(FrameError::Rekey);
        }
        let c = frame["c"].as_u64().ok_or(FrameError::Protocol)?;
        if frame["t"] != "f" || c != self.next {
            return Err(FrameError::Protocol);
        }
        let d = frame["d"].as_str().and_then(|d| unb64(d).ok()).ok_or(FrameError::Protocol)?;
        let (last, chunk) = open_frame(&self.key, c, &d).map_err(|_| FrameError::Protocol)?;
        self.next += 1;
        if self.buf.len() + chunk.len() > self.limit {
            return Err(FrameError::TooLarge);
        }
        self.buf.extend_from_slice(&chunk);
        Ok(last.then(|| std::mem::take(&mut self.buf)))
    }
}

// MARK: mailbox (§6.4)

/// Verifies and decrypts a mailbox blob sent by `dk`; returns the inner JSON bytes.
pub fn open_mailbox(
    box_secret: &StaticSecret,
    cid_raw: &[u8; 16],
    dk: &[u8; 32],
    client_nonce: &str,
    blob: &[u8],
) -> Result<Vec<u8>> {
    ensure!(blob.len() > 32 + 64 + 16, "mailbox blob too short");
    let (epk, rest) = blob.split_at(32);
    let (sig, ct) = rest.split_at(64);
    let epk: [u8; 32] = epk.try_into()?;
    let sig: [u8; 64] = sig.try_into()?;
    verify(dk, &[b"codync/mbox/v1".as_slice(), cid_raw, &epk, ct].concat(), &sig)?;
    let key = mailbox_key(&x25519(box_secret, &epk)?, cid_raw, dk, &epk);
    aead(&key)
        .decrypt(&[0u8; 12].into(), Payload { msg: ct, aad: &[dk.as_slice(), client_nonce.as_bytes()].concat() })
        .map_err(|_| anyhow!("mailbox blob doesn't decrypt"))
}

fn mailbox_key(shared: &[u8; 32], cid_raw: &[u8; 16], dk: &[u8; 32], epk: &[u8; 32]) -> [u8; 32] {
    hkdf_key(&[b"codync/mbox/v1".as_slice(), cid_raw, dk, epk].concat(), shared, b"codync/mbox-key/v1")
}

// MARK: push (§6.7)

/// Seals a notification's `{"title","body"}` JSON to a device's push key with a fresh
/// ephemeral key (`eph`; tests pass a fixed one). Returns `sealed`.
pub fn seal_push(cid_raw: &[u8; 16], push_key: &[u8; 32], plaintext: &[u8], eph: &StaticSecret) -> Result<String> {
    let epk = x25519_pub(eph);
    let shared = x25519(eph, push_key)?;
    let key =
        hkdf_key(&[b"codync/push/v1".as_slice(), cid_raw, push_key, &epk].concat(), &shared, b"codync/push-key/v1");
    let ct = aead(&key)
        .encrypt(&[0u8; 12].into(), Payload { msg: plaintext, aad: cid_raw })
        .map_err(|_| anyhow!("push payload too large"))?;
    Ok(b64(&[epk.as_slice(), &ct].concat()))
}

// MARK: SAS, offers, canonical strings (§4, §5)

pub fn sas_commit(dk: &[u8; 32], device_nonce: &[u8; 32]) -> [u8; 32] {
    sha256(&[b"codync/sascommit/v1", dk, device_nonce])
}

/// The 6-digit code both screens show.
pub fn sas_code(host_sign_pub: &[u8; 32], dk: &[u8; 32], device_nonce: &[u8; 32], host_nonce: &[u8; 32]) -> String {
    let h = sha256(&[b"codync/sas/v2", host_sign_pub, dk, device_nonce, host_nonce]);
    let n = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) % 1_000_000;
    format!("{n:06}")
}

/// The ACL `offers[].id` (and relay `pair=`) for a pairing code.
pub fn offer_id(code: &[u8; 16]) -> String {
    b64(&sha256(&[b"codync/offer/v1", code])[..16])
}

/// What the host signs to join an account (`claimSign`).
pub fn claim_input(claim_id: &str, nonce: &str, user_id: &str, computer_id: &str, box_key: &str) -> String {
    format!("codync/claim/v1\n{claim_id}\n{nonce}\n{user_id}\n{computer_id}\n{box_key}")
}

/// The `Codync-Sig` signing input.
pub fn request_sig_input(method: &str, authority: &str, path_query: &str, ts: i64, nonce: &str, body: &[u8]) -> String {
    format!(
        "codync-sig-v1\n{}\n{}\n{path_query}\n{ts}\n{nonce}\n{}",
        method.to_ascii_uppercase(),
        authority.to_ascii_lowercase(),
        b64(&sha256(&[body]))
    )
}

/// The `Codync-Sig` header value.
pub fn request_sig_header(key: &SigningKey, ts: i64, nonce: &str, input: &str) -> String {
    let kid = b64(key.verifying_key().as_bytes());
    let sig = b64(&sign(key, input.as_bytes()));
    format!("v=1,kid={kid},ts={ts},nonce={nonce},sig={sig}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::Value;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../../docs/remote-relay-vectors.json")).unwrap()
    }

    fn b(v: &Value) -> Vec<u8> {
        unb64(v.as_str().unwrap()).unwrap()
    }

    fn b32(v: &Value) -> [u8; 32] {
        unb64_n(v.as_str().unwrap()).unwrap()
    }

    struct Keys {
        host: SigningKey,
        host_box: StaticSecret,
        device: SigningKey,
    }

    fn keys(v: &Value) -> Keys {
        let k = &v["keys"];
        let host = SigningKey::from_bytes(&b32(&k["hostSignSeed"]));
        let host_box = StaticSecret::from(b32(&k["hostBoxPriv"]));
        let device = SigningKey::from_bytes(&b32(&k["deviceSignSeed"]));
        assert_eq!(b64(host.verifying_key().as_bytes()), k["hostSignPub"]);
        assert_eq!(b64(&x25519_pub(&host_box)), k["hostBoxPub"]);
        assert_eq!(b64(device.verifying_key().as_bytes()), k["deviceSignPub"]);
        assert_eq!(computer_id(host.verifying_key().as_bytes()), k["computerId"]);
        Keys { host, host_box, device }
    }

    #[test]
    fn handshake_vectors() {
        let v = vectors();
        let k = keys(&v);
        let h = &v["handshake"];
        let cid = computer_id_raw(k.host.verifying_key().as_bytes());
        let dk = k.device.verifying_key().to_bytes();
        let dev_eph = StaticSecret::from(b32(&h["deviceEphPriv"]));
        let host_eph = StaticSecret::from(b32(&h["hostEphPriv"]));
        let (ek_d, ek_h) = (x25519_pub(&dev_eph), x25519_pub(&host_eph));
        assert_eq!(b64(&ek_d), h["deviceEphPub"]);
        assert_eq!(b64(&ek_h), h["hostEphPub"]);
        let n = b32(&h["n"]);

        let hs1 = hs1_input(&cid, &dk, &ek_d, &n);
        assert_eq!(b64(&hs1), h["hs1SignInput"]);
        let sig = sign(&k.device, &hs1);
        assert_eq!(b64(&sig), h["hs1Sig"]);
        verify(&dk, &hs1, &sig).unwrap();

        let th = transcript_hash(&cid, &dk, &ek_d, &n, &ek_h);
        assert_eq!(b64(&th), h["transcriptHash"]);
        assert_eq!(b64(&sign(&k.host, &th)), h["hs2Sig"]);

        let ss = x25519(&dev_eph, &ek_h).unwrap();
        assert_eq!(ss, x25519(&host_eph, &ek_d).unwrap());
        assert_eq!(b64(&ss), h["sharedSecret"]);
        let (prk, _) = Hkdf::<Sha256>::extract(Some(&th), &ss);
        assert_eq!(b64(&prk), h["prk"]);
        let keys = channel_keys(&ss, &th);
        assert_eq!(b64(&keys.d2h), h["kD2H"]);
        assert_eq!(b64(&keys.h2d), h["kH2D"]);

        assert!(x25519(&host_eph, &[0u8; 32]).is_err(), "all-zero shared secret is refused");
        let mut tampered = hs1.clone();
        tampered[20] ^= 1;
        assert!(verify(&dk, &tampered, &sig).is_err());
    }

    #[test]
    fn frame_vectors() {
        let v = vectors();
        let h = &v["handshake"];
        let (d2h, h2d) = (b32(&h["kD2H"]), b32(&h["kH2D"]));
        for f in v["frames"].as_array().unwrap() {
            let key = if f["dir"] == "d2h" { &d2h } else { &h2d };
            let c = f["c"].as_u64().unwrap();
            let last = f["final"].as_bool().unwrap();
            let msg = f["message"].as_str().unwrap().as_bytes();
            let plaintext = b(&f["plaintext"]);
            assert_eq!(plaintext[0], u8::from(!last));
            assert_eq!(&plaintext[1..], msg);
            let d = seal_frame(key, c, last, msg);
            assert_eq!(b64(&d), f["d"]);
            assert_eq!(open_frame(key, c, &d).unwrap(), (last, msg.to_vec()));
            assert!(open_frame(key, c + 1, &d).is_err(), "the counter is bound to the frame");
        }
    }

    #[test]
    fn sealer_and_opener_chunk_count_and_limit() {
        let key = [7u8; 32];
        let mut sealer = Sealer::new(key);
        let mut opener = Opener::new(key, 1024 * 1024);
        let big = vec![b'x'; CHUNK * 2 + 5];
        let frames = sealer.seal(&big).unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(opener.open(&frames[0]), Ok(None));
        assert_eq!(opener.open(&frames[1]), Ok(None));
        assert_eq!(opener.open(&frames[2]), Ok(Some(big)));

        let empty = sealer.seal(b"").unwrap();
        assert_eq!(opener.open(&empty[0]), Ok(Some(vec![])));

        // Replay and reordering are both a counter mismatch.
        let a = sealer.seal(b"a").unwrap();
        let b = sealer.seal(b"b").unwrap();
        assert_eq!(opener.open(&b[0]), Err(FrameError::Protocol));
        let mut small = Opener::new(key, 4);
        let mut s2 = Sealer::new(key);
        assert_eq!(small.open(&s2.seal(b"12345").unwrap()[0]), Err(FrameError::TooLarge));
        drop(a);

        let mut near_end = Sealer { key, next: REKEY_AT - 1 };
        assert!(near_end.seal(b"x").is_some());
        assert!(near_end.seal(b"x").is_none(), "no frame may use counter 2^32");
    }

    #[test]
    fn mailbox_vector_opens() {
        let v = vectors();
        let k = keys(&v);
        let m = &v["mailbox"];
        let cid = computer_id_raw(k.host.verifying_key().as_bytes());
        let dk = k.device.verifying_key().to_bytes();
        let eph = StaticSecret::from(b32(&m["ephPriv"]));
        let epk = x25519_pub(&eph);
        assert_eq!(b64(&epk), m["ephPub"]);
        let ss = x25519(&k.host_box, &epk).unwrap();
        assert_eq!(b64(&ss), m["sharedSecret"]);
        assert_eq!(b64(&mailbox_key(&ss, &cid, &dk, &epk)), m["key"]);
        let blob = b(&m["blob"]);
        assert_eq!(blob, [epk.as_slice(), &b(&m["sig"]), &b(&m["ciphertext"])].concat());

        let nonce = m["clientNonce"].as_str().unwrap();
        let plain = open_mailbox(&k.host_box, &cid, &dk, nonce, &blob).unwrap();
        assert_eq!(plain, m["plaintext"].as_str().unwrap().as_bytes());

        assert!(open_mailbox(&k.host_box, &cid, &dk, "other-nonce", &blob).is_err(), "nonce is authenticated");
        let mut tampered = blob.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(open_mailbox(&k.host_box, &cid, &dk, nonce, &tampered).is_err(), "signature covers ct");
        let other = SigningKey::from_bytes(&[9u8; 32]).verifying_key().to_bytes();
        assert!(open_mailbox(&k.host_box, &cid, &other, nonce, &blob).is_err(), "sender is authenticated");
    }

    #[test]
    fn push_vector_seals() {
        let v = vectors();
        let k = keys(&v);
        let p = &v["push"];
        let cid = computer_id_raw(k.host.verifying_key().as_bytes());
        let push_priv = StaticSecret::from(b32(&p["pushPriv"]));
        let push_key = x25519_pub(&push_priv);
        assert_eq!(b64(&push_key), p["pushPub"]);
        let eph = StaticSecret::from(b32(&p["ephPriv"]));
        assert_eq!(b64(&x25519_pub(&eph)), p["ephPub"]);
        let ss = x25519(&eph, &push_key).unwrap();
        assert_eq!(b64(&ss), p["sharedSecret"]);
        let sealed = seal_push(&cid, &push_key, p["plaintext"].as_str().unwrap().as_bytes(), &eph).unwrap();
        assert_eq!(sealed, p["sealed"]);
    }

    #[test]
    #[allow(clippy::many_single_char_names)] // vector sections, named as in the JSON
    fn sas_offer_claim_and_request_sig_vectors() {
        let v = vectors();
        let k = keys(&v);
        let dk = k.device.verifying_key().to_bytes();
        let s = &v["sas"];
        let (nd, nh) = (b32(&s["deviceNonce"]), b32(&s["hostNonce"]));
        assert_eq!(b64(&sas_commit(&dk, &nd)), s["commit"]);
        assert_eq!(sas_code(k.host.verifying_key().as_bytes(), &dk, &nd, &nh), s["code"]);

        let p = &v["pairing"];
        assert_eq!(offer_id(&unb64_n(p["code"].as_str().unwrap()).unwrap()), p["offerId"]);

        let c = &v["claim"];
        let input = claim_input(
            c["claimId"].as_str().unwrap(),
            c["nonce"].as_str().unwrap(),
            c["userId"].as_str().unwrap(),
            c["computerId"].as_str().unwrap(),
            c["boxKey"].as_str().unwrap(),
        );
        assert_eq!(input, c["canonical"]);
        assert_eq!(b64(&sign(&k.host, input.as_bytes())), c["sig"]);

        let r = &v["requestSig"];
        assert_eq!(b64(&sha256(&[b""])), r["bodySha256"]);
        let ts = r["ts"].as_i64().unwrap();
        let nonce = r["nonce"].as_str().unwrap();
        let input = request_sig_input(
            r["method"].as_str().unwrap(),
            r["authority"].as_str().unwrap(),
            r["pathAndQuery"].as_str().unwrap(),
            ts,
            nonce,
            b"",
        );
        assert_eq!(input, r["canonical"]);
        let header = request_sig_header(&k.device, ts, nonce, &input);
        assert_eq!(format!("Codync-Sig: {header}"), r["header"]);

        let a = &v["acl"];
        let json = a["json"].as_str().unwrap();
        assert_eq!(b64(json.as_bytes()), a["d"]);
        assert_eq!(b64(&sign(&k.host, json.as_bytes())), a["sig"]);
    }
}
