# n8n-nodes-codync

The Codync node for the n8n instance the Codync host runs: the Codync bots on
this computer, from n8n workflows.

## How it gets installed

The host writes this package to the n8n instance's `custom/node_modules/`
folder before starting n8n (the custom extensions folder is always scanned), so
there is nothing to install and no environment to configure:

```
<user folder>/custom/node_modules/n8n-nodes-codync/
├── package.json
└── dist/
    ├── CodyncBot.node.js
    └── CodyncApi.credentials.js
```

In the editor the node is **Codync Bot** (type `CUSTOM.CodyncBot`), with an
optional **Codync API** credential. The host starts n8n with
`CODYNC_HOST_URL` and `CODYNC_HOST_TOKEN` in its environment, so with no
credential selected the node talks to this computer's host automatically.

## Operations

| Operation            | What it does                                                              |
| -------------------- | ------------------------------------------------------------------------- |
| Send a message       | `POST /api/send`, then (by default) polls `messageResult` every 2 s until the turn ends or the timeout; the bot's reply comes out on the item. A bot waiting for a sign-in keeps polling. |
| List bots            | The agents the roster shows (groups, hidden and deleted bots stay out).    |
| Read history         | The bot's recent entries (id, seq, kind, text, createdAt).                 |
| Run a routine        | Fires the routine now, disabled ones included, and returns the run.        |
| Stop a bot           | Stops the bot's running turn.                                              |
| Start a new session  | Resets the bot's agent session.                                            |

The Bot and Routine dropdowns load live from the host (a routine list follows
the selected bot).
