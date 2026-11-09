# oh-my-pi

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Tool call titles are prose ("Creating probe bot"), not the tool's name, so Codync's own calls aren't recognized as its bookkeeping | **Not patched** (`host/src/agent/bot/tool_entry.rs` matches names only) | none | Titles or `_meta` carry the tool's name |
