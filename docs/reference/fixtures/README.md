# Shared cryptographic fixtures

[remote-relay-vectors.json](remote-relay-vectors.json) contains deterministic protocol-v1 vectors for host/device keys, handshake, frame encryption, chunking, SAS, pairing offers, mailbox, request signatures, claims, signed ACL and sealed push.

The private values are fixed **test seeds**, not live credentials. Changing these bytes changes the common contract checked by Rust, Swift and TypeScript.

[remote-relay-vectors.py](remote-relay-vectors.py) prints the JSON to stdout and requires Python 3 plus `cryptography`. To compare without overwriting the checked-in fixture, from the repository root:

```sh
python3 docs/reference/fixtures/remote-relay-vectors.py > /tmp/codync-relay-vectors.json
cmp docs/reference/fixtures/remote-relay-vectors.json /tmp/codync-relay-vectors.json
```

Consumers:

- `host/src/crypto.rs` (`include_str!`)
- `kit/Tests/CodyncKitTests/RelayVectorsTests.swift`
- `cloud/test/vectors.test.ts` and `cloud/test/relay.test.ts`

When moving or changing a fixture, update those paths and the cloud/kit/host CI path filters. Run the affected vector tests; a Markdown link check alone cannot validate executable imports. [Protocol reference](../remote-relay.md).
