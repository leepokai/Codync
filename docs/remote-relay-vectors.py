# Generates docs/remote-relay-vectors.json (Codync relay crypto, protocol v1 (spec rev 2)).
import base64, hashlib, json, struct
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives.kdf.hkdf import HKDF, HKDFExpand
from cryptography.hazmat.primitives import hashes, hmac
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

b64 = lambda b: base64.urlsafe_b64encode(b).rstrip(b"=").decode()
raw = lambda k: k.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
sha = lambda b: hashlib.sha256(b).digest()

def extract(salt, ikm):
    h = hmac.HMAC(salt, hashes.SHA256()); h.update(ikm); return h.finalize()
def expand(prk, info, n=32):
    return HKDFExpand(hashes.SHA256(), n, info).derive(prk)

host_sign = Ed25519PrivateKey.from_private_bytes(b"\x01"*32)
host_box = X25519PrivateKey.from_private_bytes(b"\x02"*32)
dev_sign = Ed25519PrivateKey.from_private_bytes(b"\x03"*32)
dev_eph = X25519PrivateKey.from_private_bytes(b"\x04"*32)
host_eph = X25519PrivateKey.from_private_bytes(b"\x05"*32)
n = b"\x06"*32
mbox_eph = X25519PrivateKey.from_private_bytes(b"\x07"*32)
code = b"\x08"*16
sig_nonce = b"\x09"*16

hk, bk, dk, ekd, ekh = raw(host_sign), raw(host_box), raw(dev_sign), raw(dev_eph), raw(host_eph)
cid = sha(hk)[:16]

hs1_input = b"codync/hs1/v1" + cid + dk + ekd + n
hs1_sig = dev_sign.sign(hs1_input)
th = sha(b"codync/hs2/v1" + cid + dk + ekd + n + ekh)
hs2_sig = host_sign.sign(th)
shared = dev_eph.exchange(X25519PublicKey.from_public_bytes(ekh))
assert shared == host_eph.exchange(X25519PublicKey.from_public_bytes(ekd))
prk = extract(th, shared)
k_d2h = expand(prk, b"codync/d2h/v1")
k_h2d = expand(prk, b"codync/h2d/v1")

def frame(key, counter, flag, msg):
    nonce = b"\x00"*4 + struct.pack(">Q", counter)
    aad = b"codync/frame/v1" + struct.pack(">Q", counter)
    pt = bytes([flag]) + msg.encode()
    return b64(ChaCha20Poly1305(key).encrypt(nonce, pt, aad)), b64(pt)

req = '{"id":1,"m":"hello","b":{}}'
res = '{"id":1,"ok":{"name":"Mac"}}'
f_d2h0 = frame(k_d2h, 0, 0, req)
f_h2d0 = frame(k_h2d, 0, 0, res)
f_d2h1_more = frame(k_d2h, 1, 1, '{"id":2,"m":"se')

# SAS: commit-then-reveal (device commits to nd before it sees nh)
nd = b"\x0a"*32
nh = b"\x0b"*32
sas_commit = sha(b"codync/sascommit/v1" + dk + nd)
sas_n = struct.unpack(">I", sha(b"codync/sas/v2" + hk + dk + nd + nh)[:4])[0] % 1_000_000
offer = sha(b"codync/offer/v1" + code)[:16]

# mailbox
epk = raw(mbox_eph)
mshared = mbox_eph.exchange(X25519PublicKey.from_public_bytes(bk))
client_nonce = "3F2504E0-4F89-11D3-9A0C-0305E82C3301"
mprk = extract(b"codync/mbox/v1" + cid + dk + epk, mshared)
mkey = expand(mprk, b"codync/mbox-key/v1")
mpt = '{"m":"send","b":{"botId":"b1","text":"hi","clientNonce":"%s"},"ts":1790000000000}' % client_nonce
mct = ChaCha20Poly1305(mkey).encrypt(b"\x00"*12, mpt.encode(), dk + client_nonce.encode())
msig = dev_sign.sign(b"codync/mbox/v1" + cid + epk + mct)
blob = epk + msig + mct

# Codync-Sig request signing
path = "/v1/relay/device/%s?v=1" % b64(cid)
ts = 1790000000000
authority = "codync-cloud-staging.example.workers.dev"
canon = "codync-sig-v1\nGET\n%s\n%s\n%d\n%s\n%s" % (authority, path, ts, b64(sig_nonce), b64(sha(b"")))
req_sig = dev_sign.sign(canon.encode())
header = "v=1,kid=%s,ts=%d,nonce=%s,sig=%s" % (b64(dk), ts, b64(sig_nonce), b64(req_sig))

# ACL
acl = json.dumps({"v":1,"computerId":b64(cid),"ver":1,"devices":[{"dk":b64(dk),"grant":None}],"offers":[{"id":b64(offer),"exp":ts+600000}]}, separators=(",",":"))
acl_sig = host_sign.sign(acl.encode())

# claim signature (UTF-8, "\n"-separated, no trailing newline)
claim_canon = "codync/claim/v1\n%s\n%s\n%s\n%s\n%s" % ("clm_AAAAAAAAAAAAAAAAAAAAAA", "DAwMDAwMDAwMDAwMDAwMDA", "user_2abc", b64(cid), b64(bk))
claim_sig = host_sign.sign(claim_canon.encode())

# sealed push payload (host -> phone via APNs, opened in the Notification Service Extension)
push_priv = X25519PrivateKey.from_private_bytes(b"\x0c"*32)
push_pub = raw(push_priv)
push_eph = X25519PrivateKey.from_private_bytes(b"\x0d"*32)
pepk = raw(push_eph)
pshared = push_eph.exchange(X25519PublicKey.from_public_bytes(push_pub))
pprk = extract(b"codync/push/v1" + cid + push_pub + pepk, pshared)
pkey = expand(pprk, b"codync/push-key/v1")
ppt = '{"title":"Reviewer","body":"Run cargo test --all?"}'
pct = ChaCha20Poly1305(pkey).encrypt(b"\x00"*12, ppt.encode(), cid)

out = {
  "about": "Codync relay protocol v1 (spec rev 2) test vectors. All binary values are base64url without padding. Private keys are 32-byte seeds/scalars.",
  "keys": {
    "hostSignSeed": b64(b"\x01"*32), "hostSignPub": b64(hk),
    "hostBoxPriv": b64(b"\x02"*32), "hostBoxPub": b64(bk),
    "deviceSignSeed": b64(b"\x03"*32), "deviceSignPub": b64(dk),
    "computerId": b64(cid),
  },
  "handshake": {
    "deviceEphPriv": b64(b"\x04"*32), "deviceEphPub": b64(ekd),
    "hostEphPriv": b64(b"\x05"*32), "hostEphPub": b64(ekh),
    "n": b64(n),
    "hs1SignInput": b64(hs1_input), "hs1Sig": b64(hs1_sig),
    "transcriptHash": b64(th), "hs2Sig": b64(hs2_sig),
    "sharedSecret": b64(shared), "prk": b64(prk),
    "kD2H": b64(k_d2h), "kH2D": b64(k_h2d),
  },
  "frames": [
    {"dir":"d2h","c":0,"final":True,"message":req,"plaintext":f_d2h0[1],"d":f_d2h0[0]},
    {"dir":"h2d","c":0,"final":True,"message":res,"plaintext":f_h2d0[1],"d":f_h2d0[0]},
    {"dir":"d2h","c":1,"final":False,"message":'{"id":2,"m":"se',"plaintext":f_d2h1_more[1],"d":f_d2h1_more[0]},
  ],
  "sas": {"deviceNonce": b64(nd), "hostNonce": b64(nh), "commit": b64(sas_commit), "code": "%06d" % sas_n},
  "claim": {"claimId": "clm_AAAAAAAAAAAAAAAAAAAAAA", "nonce": "DAwMDAwMDAwMDAwMDAwMDA", "userId": "user_2abc",
            "computerId": b64(cid), "boxKey": b64(bk), "canonical": claim_canon, "sig": b64(claim_sig)},
  "push": {"pushPriv": b64(b"\x0c"*32), "pushPub": b64(push_pub), "ephPriv": b64(b"\x0d"*32), "ephPub": b64(pepk),
           "sharedSecret": b64(pshared), "key": b64(pkey), "plaintext": ppt, "sealed": b64(pepk + pct)},
  "pairing": {"code": b64(code), "offerId": b64(offer)},
  "mailbox": {
    "ephPriv": b64(b"\x07"*32), "ephPub": b64(epk), "clientNonce": client_nonce,
    "sharedSecret": b64(mshared), "key": b64(mkey),
    "plaintext": mpt, "ciphertext": b64(mct), "sig": b64(msig), "blob": b64(blob),
  },
  "requestSig": {
    "method":"GET","authority":authority,"pathAndQuery":path,"ts":ts,"nonce":b64(sig_nonce),"bodySha256":b64(sha(b"")),
    "canonical": canon, "sig": b64(req_sig), "header": "Codync-Sig: " + header,
  },
  "acl": {"json": acl, "d": b64(acl.encode()), "sig": b64(acl_sig)},
}
print(json.dumps(out, indent=2))
