# Third-party notices

Codync's own license is in `LICENSE` and `NOTICE`. Where a file adapts or ports code from another project, the file's
header names it, and the project's notice is kept here.

## Mercury Agent

Used in: `host/src/agent/blocklist.rs` (the list of forbidden shell commands, ported from
`src/capabilities/shell/blocklist.ts`, with changes: literal-star semantics for the root patterns, extra variants, and
chain and wrapper handling).

Source: https://github.com/cosmicstack-labs/mercury-agent

MIT License

Copyright (c) 2026 Cosmic Stack

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

## Rakazo

Used in: `apps/desktop/src/shared/agent-import.ts` and `host/src/chat/memory/agent_import.rs` (importing the folder of an
OpenClaw or Hermes agent: the file lists of each format, the credential shapes, Hermes' `§` memory separator and bundled
skill manifest, OpenClaw's identity parsing). Ported in design from `packages/core/src/agent-import/`; the code is
rewritten, not copied, and changed from the original: one small module, no archive reader, no Rakazo contracts; the
Rust side re-implements the credential shapes without regular expressions.

Source: Rakazo, `packages/core/src/agent-import/`

Licensed under the Apache License, Version 2.0 (https://www.apache.org/licenses/LICENSE-2.0). The original has no NOTICE
file; this entry is the attribution. Unless required by applicable law or agreed to in writing, software distributed under
the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND.
