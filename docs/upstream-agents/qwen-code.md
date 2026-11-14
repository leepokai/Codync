# Qwen Code

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Terminal sign-in in `_meta: {type: "terminal", args}` instead of the spec's fields | `host/src/agent/auth.rs` reads it | none | The spec's fields |
