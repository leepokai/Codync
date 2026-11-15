# Grok Build

Rules: [README](README.md).

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| Not every build answers ACP's extension name `_x.ai/billing`, and the 1.0.75 binary names `x.ai/billing` | `host/src/usage/grok.rs`: asks `x.ai/billing`, then `_x.ai/billing` | No issue opened: seen only in the binary's strings, with no login to run it | The oldest build we support answers one name |
