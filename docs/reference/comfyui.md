# ComfyUI (bundled tool)

How ComfyUI ships and runs inside Codync, what was taken from its repository, and where Codync differs from the plan (`docs/plans/bundled-tools-production.md`).

## One pin

`packaging/comfy/version.json` is the only place the version is named: repo, tag (`v0.44.0`), commit (`2c75dc1b…`), release asset, size and SHA-256. `packaging/comfy/fetch.mjs` (packaging) and `host/src/tools/comfy.rs` (the card's version) both read it. To update ComfyUI: change that file from the release page, nothing else.

## What is taken from the official repository (Comfy-Org/ComfyUI @ v0.44.0)

| Piece | Source | Used for |
| --- | --- | --- |
| `ComfyUI_windows_portable_nvidia.7z` | the release asset, checked against the pinned size and SHA-256 | the program: `ComfyUI/` + `python_embeded/`, unpacked to `build/native/comfy` and shipped as `resources/comfy` |
| Backend, unchanged | `main.py` and everything it loads | started as `python_embeded/python.exe -s ComfyUI/main.py …`; no file of it is copied into Codync |
| CLI flags | `comfy/cli_args.py`: `--listen`, `--port`, `--disable-auto-launch`, `--user-directory`, `--input-directory`, `--output-directory`, `--temp-directory`, `--extra-model-paths-config`, `--cpu` | `tools/comfy.rs` `launch` |
| Folder keys | `extra_model_paths.yaml.example` | the generated `~/.codync/comfy/extra_model_paths.yaml` (`MODEL_FOLDERS`) |
| HTTP API | `server.py`: `/system_stats`, `/object_info`, `/prompt`, `/queue`, `/history/{id}`, `/interrupt`, `/view`, `/upload/image` | `tools/comfy_api.rs` |
| Example workflow | `script_examples/basic_api_example.py`, the graph in `prompt_text`, copied verbatim | `tools/comfy_example.json` (the `example_workflow` tool) |

The only copied content is the example graph: its JSON is identical to the original's (`diff` of the two JSON values is empty; Codync only reflows it onto one line per node).

## Where this differs from the plan

- **No multi-user mode, no seeded user.** In v0.44.0 `app/user_manager.py` has no tokens: with `--multi-user` a user is an id sent in the `comfy-user` header and the frontend shows a user picker when none is stored; without it there is the single `default` user, no sign-in screen at all, and the editor's settings are stored on the server under `--user-directory`. Seeding `local@codync` would add the very picker the plan forbids and no security (the server listens on loopback only), so Codync runs single-user. There is nothing for the vault to hold and no session to retry.
- **Node names are checked against `/object_info`, not `nodes.py`.** The running server is the truth for built-in and custom nodes alike (`comfy_job::validate`); a copy of `nodes.py` would go stale.
- **The repo has no `user/default/workflows`** in this version; the example is the repo's API example above.
- **Windows x64 only** (the NVIDIA portable also runs on the CPU). Other targets get an empty `comfy/` folder and the card says the build does not include ComfyUI.

## Runtime

`host/src/tools/` is the shared runner (see its `mod.rs`): `runner.rs` starts the process inside a Job Object (`agent::process::Process`, so stopping it ends Python and everything it started), picks the port (8188, else a free one, noted in the log), keeps the state `stopped/starting/running/error`, and watches for a crash; `health.rs` probes `GET /system_stats`; `log.rs` writes `~/.codync/logs/comfy.log` (rotated at 5 MB); `lifecycle.rs` starts what was switched on with the host and stops everything on shutdown. n8n still runs through `host/src/n8n.rs`.

Data: `~/.codync/comfy/{user,models,custom_nodes,input,output,temp}` (`~/.codync-dev` for Codync Dev). The program under `resources/` is replaced whole on update; the data folder is never touched. Both `electron-builder.yml` and `electron-builder.dev.yml` list `build/native/comfy` because the Dev config replaces the base resource list.

## Bots

The host registers a `comfyui` connector while ComfyUI runs (`lifecycle::offer_to_bots`); a bot gets the `codync-host mcp comfy` server only when its owner turned that connector on (`agent/bot/session.rs`). Tools: `list_models`, `list_nodes`, `example_workflow`, `run_workflow`, `wait_for_job`, `upload_image`, `job_status`, `cancel_job`. The host follows each job (`/history`, `/queue`, 30 minute limit) for the card. The images reach the bot's chat with `send_file`, which only works while the bot's turn runs, so the bot's own call delivers them: `run_workflow` queues and waits up to 5 minutes; a longer job returns still running and `wait_for_job` (or a `job_status` look) delivers it later, once. A wait abandoned because the asking bot's turn stopped cancels the job (`comfy_job::CancelOnDrop`); another bot's wait never does. One job at a time. Host API: `toolStatus|toolStart|toolStop` (`{id}`), `comfyCancel`, `comfyJobImage`; `comfyCall` (the MCP bridge) is loopback-only, like the other bridges.

## Known limits

- The first start needs about 10 GB free; models are extra.
- The portable is about 2 GB to download and 5+ GB unpacked; the Windows installer grows accordingly.
- Windows may ask for a firewall rule if something outside loopback is attempted; ComfyUI is started with `--listen 127.0.0.1`, so none is expected.
- Uninstalling keeps `~/.codync/comfy` unless the user answers Yes to "Also delete your ComfyUI models…" (`resources/installer.nsh` runs `codync-host uninstall --remove-comfy`; the host knows which data folder is this app's, Dev or production; a silent uninstall keeps it). Updating never touches it.
- Not covered here: macOS and Linux, several jobs in parallel.
