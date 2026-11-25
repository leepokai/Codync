# iMate

Rules: [README](README.md).

iMate's open platform (`https://imate.woa.com`) has no public issue tracker we can file these on. The bridge is `host/src/agent/imate_bridge.rs`.

| Defect | Codync's patch | Upstream | Drop when |
|---|---|---|---|
| An HTTP body is sometimes two JSON objects on separate lines, the ACP frame and then a framework suffix such as `{"code":0,"message":""}`. Decoding the whole body fails, and the suffix is not a frame the host should see | `first_line` / `relay_http_body`: keep the first line, and forward it only when it is a JSON-RPC frame | none: no public tracker | Keep until a response is one JSON value |
| `session/new`, `session/load` and `session/resume` require `mcpServers` (omitting it is `Invalid params`, -32602) and reject `additionalDirectories`. The servers are processes on this computer, which iMate cannot start | `forward_frame` sends `mcpServers: []` and drops `additionalDirectories` | none: no public tracker | Keep until iMate accepts a missing `mcpServers` and ignores unknown directory fields |
