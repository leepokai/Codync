/**
 * Codync Bot node: the Codync bots on this computer, from n8n workflows.
 *
 * The Codync host that runs n8n also runs the bots, and starts this process
 * with CODYNC_HOST_URL and CODYNC_HOST_TOKEN, so the node works with an
 * unconfigured credential. Every operation talks to the host over its local
 * API (`POST /api/<method>`, bearer token, loopback only).
 */

const POLL_INTERVAL_MS = 2000;
const FINAL_STATES = new Set(['done', 'failed', 'stopped', 'incomplete', 'interrupted']);

async function auth(context) {
    let hostUrl = '';
    let token = '';
    try {
        const credentials = await context.getCredentials('codyncApi');
        hostUrl = String(credentials.hostUrl || '').trim();
        token = String(credentials.token || '').trim();
    } catch {
        // No credential selected: use what the Codync host injected.
    }
    hostUrl = hostUrl || process.env.CODYNC_HOST_URL || '';
    token = token || process.env.CODYNC_HOST_TOKEN || '';
    if (!hostUrl) {
        throw new Error(
            'The Codync host URL is not set. Start n8n from Codync, or fill in the Codync API credential.',
        );
    }
    if (!token) {
        throw new Error(
            'The Codync host token is not set. Start n8n from Codync, or fill in the Codync API credential.',
        );
    }
    return { hostUrl: hostUrl.replace(/\/+$/, ''), token };
}

async function api(context, method, params) {
    const { hostUrl, token } = await auth(context);
    let response;
    try {
        response = await fetch(`${hostUrl}/api/${method}`, {
            method: 'POST',
            headers: {
                'content-type': 'application/json',
                authorization: `Bearer ${token}`,
            },
            body: JSON.stringify(params ?? {}),
        });
    } catch (error) {
        throw new Error(`Could not reach the Codync host at ${hostUrl}: ${error.message}`);
    }
    const body = await response.json().catch(() => ({}));
    if (!response.ok) {
        const detail = typeof body.error === 'string' ? body.error : JSON.stringify(body);
        throw new Error(`Codync ${method} failed (${response.status}): ${detail}`);
    }
    return body;
}

async function waitForReply(context, botId, entryId, timeoutSeconds) {
    const deadline = Date.now() + timeoutSeconds * 1000;
    for (;;) {
        const body = await api(context, 'messageResult', { botId, id: entryId });
        const result = body.result;
        const state = result && typeof result.state === 'string' ? result.state : '';
        if (FINAL_STATES.has(state)) {
            return result;
        }
        // No result yet, or the bot is waiting for a sign-in: keep polling.
        if (timeoutSeconds > 0 && Date.now() >= deadline) {
            return {
                state: 'timeout',
                text: result && typeof result.text === 'string' ? result.text : '',
                stopReason: 'timeout',
            };
        }
        await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
    }
}

function textOf(entry) {
    const data = entry && entry.data;
    if (data && typeof data.text === 'string' && data.text) {
        return data.text;
    }
    if (data && Array.isArray(data.attachments)) {
        const names = data.attachments.map((a) => (a && a.name) || '').filter(Boolean);
        if (names.length > 0) {
            return names.join(', ');
        }
    }
    return '';
}

class CodyncBot {
    description = {
        displayName: 'Codync Bot',
        name: 'CodyncBot',
        version: 1,
        group: ['transform'],
        description:
            'Talk to the Codync bots on this computer: send messages, read history and run routines',
        defaults: {
            name: 'Codync Bot',
        },
        inputs: ['main'],
        outputs: ['main'],
        credentials: [
            {
                name: 'codyncApi',
                required: false,
            },
        ],
        properties: [
            {
                displayName: 'Operation',
                name: 'operation',
                type: 'options',
                noDataExpression: true,
                default: 'sendMessage',
                options: [
                    { name: 'Send a message', value: 'sendMessage' },
                    { name: 'List bots', value: 'listBots' },
                    { name: 'Read history', value: 'history' },
                    { name: 'Run a routine', value: 'runRoutine' },
                    { name: 'Stop a bot', value: 'stopBot' },
                    { name: 'Start a new session', value: 'newSession' },
                ],
            },
            {
                displayName: 'Bot',
                name: 'botId',
                type: 'options',
                description: 'The bot to talk to',
                options: [],
                default: '',
                required: true,
                typeOptions: { loadOptionsMethod: 'bots' },
                displayOptions: {
                    show: {
                        operation: ['sendMessage', 'history', 'runRoutine', 'stopBot', 'newSession'],
                    },
                },
            },
            {
                displayName: 'Message',
                name: 'message',
                type: 'string',
                description: 'What to say to the bot',
                default: '',
                required: true,
                typeOptions: { rows: 3 },
                displayOptions: {
                    show: { operation: ['sendMessage'] },
                },
            },
            {
                displayName: 'Wait for the reply',
                name: 'waitForReply',
                type: 'boolean',
                default: true,
                description: 'Poll until the bot finishes the turn and pass its reply on',
                displayOptions: {
                    show: { operation: ['sendMessage'] },
                },
            },
            {
                displayName: 'Timeout (seconds)',
                name: 'timeoutSeconds',
                type: 'number',
                default: 300,
                description:
                    'Give up waiting this long and pass on what the bot said so far (state "timeout")',
                displayOptions: {
                    show: { operation: ['sendMessage'], waitForReply: [true] },
                },
            },
            {
                displayName: 'Entries',
                name: 'limit',
                type: 'number',
                default: 20,
                placeholder: '1-500',
                description: 'How many recent entries to read',
                displayOptions: {
                    show: { operation: ['history'] },
                },
            },
            {
                displayName: 'Routine',
                name: 'routineId',
                type: 'options',
                description: 'The routine to run now',
                options: [],
                default: '',
                required: true,
                typeOptions: {
                    loadOptionsMethod: 'routines',
                    loadOptionsDependsOn: ['botId'],
                },
                displayOptions: {
                    show: { operation: ['runRoutine'] },
                },
            },
        ],
    };

    methods = {
        loadOptions: {
            bots: async function () {
                const body = await api(this, 'botList');
                return (body.bots || []).map((bot) => ({
                    name: bot.name,
                    value: bot.id,
                    ...(bot.external ? { description: 'Runs in a terminal' } : {}),
                }));
            },
            routines: async function () {
                const botId = this.getNodeParameter('botId', 0);
                if (!botId) {
                    return [];
                }
                const body = await api(this, 'routines', { botId });
                return (body.routines || []).map((routine) => ({
                    name: routine.name,
                    value: routine.id,
                    ...(routine.enabled ? {} : { description: 'Disabled' }),
                }));
            },
        },
    };

    async execute() {
        const operation = this.getNodeParameter('operation', 0);
        switch (operation) {
            case 'listBots':
                return this.listBots();
            case 'history':
                return this.history();
            case 'runRoutine':
                return this.runRoutine();
            case 'stopBot':
                return this.stopBot();
            case 'newSession':
                return this.newSession();
            case 'sendMessage':
                return this.sendMessage();
            default:
                throw new Error(`Unknown operation "${operation}"`);
        }
    }

    async listBots() {
        const body = await api(this, 'botList');
        return (body.bots || []).map((bot) => ({
            json: {
                botId: bot.id,
                name: bot.name,
                description: bot.description || '',
                external: bot.external,
            },
        }));
    }

    async history() {
        const botId = this.getNodeParameter('botId', 0);
        const limit = Math.min(
            Math.max(Number(this.getNodeParameter('limit', 0, { fallbackValue: 20 })) || 20, 1),
            500,
        );
        const body = await api(this, 'history', { botId, limit });
        return (body.entries || []).map((entry) => ({
            json: {
                botId,
                id: entry.id,
                seq: entry.seq,
                kind: entry.kind,
                text: textOf(entry),
                createdAt: entry.createdAt,
            },
        }));
    }

    async runRoutine() {
        const botId = this.getNodeParameter('botId', 0);
        const routineId = this.getNodeParameter('routineId', 0);
        const body = await api(this, 'runRoutine', { botId, id: routineId });
        return [{ json: { botId, routineId, run: body.run || {} } }];
    }

    async stopBot() {
        const botId = this.getNodeParameter('botId', 0);
        await api(this, 'stop', { botId });
        return [{ json: { botId, stopped: true } }];
    }

    async newSession() {
        const botId = this.getNodeParameter('botId', 0);
        await api(this, 'newSession', { botId });
        return [{ json: { botId, newSession: true } }];
    }

    async sendMessage() {
        const items = (await this.getInputData()) || [];
        const output = [];
        for (let i = 0; i < items.length; i += 1) {
            const botId = this.getNodeParameter('botId', i);
            const message = this.getNodeParameter('message', i);
            const body = await api(this, 'send', { botId, text: message });
            const entry = body.entry || {};
            const item = { botId, message, entryId: entry.id, state: 'sent' };
            if (this.getNodeParameter('waitForReply', i, { fallbackValue: true })) {
                const timeout =
                    Number(this.getNodeParameter('timeoutSeconds', i, { fallbackValue: 300 })) || 0;
                const result = await waitForReply(this, botId, entry.id, timeout);
                item.state = result.state;
                item.reply = typeof result.text === 'string' ? result.text : '';
                item.stopReason = result.stopReason || '';
            }
            output.push({ json: item });
        }
        return output;
    }
}

module.exports = { CodyncBot };
