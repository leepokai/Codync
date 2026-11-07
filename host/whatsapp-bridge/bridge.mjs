// Codync's WhatsApp link, as a child process of the host.
//
// One line of JSON per command in, one per event out. The host owns the bot and the chats;
// this process owns only the linked-device socket, so a WhatsApp failure never takes the host down.
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { createInterface } from 'node:readline'
import makeWASocket, {
    DisconnectReason,
    downloadMediaMessage,
    jidNormalizedUser,
    useMultiFileAuthState,
} from 'baileys'

const root = process.argv[2]
if (!root) {
    console.error('usage: node bridge.mjs <data folder>')
    process.exit(1)
}
mkdirSync(root, { recursive: true })

let socket = null
let starting = false

const out = (event) => process.stdout.write(`${JSON.stringify(event)}\n`)

function textOf(message) {
    const m = message?.message
    if (!m) return ''
    return (
        m.conversation ??
        m.extendedTextMessage?.text ??
        m.imageMessage?.caption ??
        m.videoMessage?.caption ??
        m.documentMessage?.caption ??
        ''
    )
}

function mediaOf(message) {
    const m = message?.message
    const media = m?.imageMessage ?? m?.videoMessage ?? m?.audioMessage ?? m?.documentMessage
    if (!media) return null
    return {
        kind: m?.imageMessage ? 'image' : m?.videoMessage ? 'video' : m?.audioMessage ? 'audio' : 'document',
        name: media.fileName ?? null,
        mimetype: media.mimetype ?? null,
        size: Number(media.fileLength ?? 0),
    }
}

async function start() {
    if (starting || socket) return out({ type: 'state', state: socket ? 'connected' : 'starting' })
    starting = true
    try {
        const { state, saveCreds } = await useMultiFileAuthState(join(root, 'auth'))
        socket = makeWASocket({ auth: state, printQRInTerminal: false, syncFullHistory: false, markOnlineOnConnect: false })
        socket.ev.on('creds.update', saveCreds)
        socket.ev.on('connection.update', (update) => {
            const { connection, lastDisconnect, qr } = update
            if (qr) out({ type: 'qr', qr })
            if (connection === 'open') {
                out({ type: 'state', state: 'connected', me: socket.user?.id ? jidNormalizedUser(socket.user.id) : null, name: socket.user?.name ?? null })
            }
            if (connection === 'close') {
                const code = lastDisconnect?.error?.output?.statusCode
                socket = null
                if (code === DisconnectReason.loggedOut) out({ type: 'state', state: 'loggedOut' })
                else {
                    out({ type: 'state', state: 'reconnecting' })
                    setTimeout(() => void start(), 2000)
                }
            }
        })
        socket.ev.on('messages.upsert', async ({ messages, type }) => {
            if (type !== 'notify') return
            const me = socket?.user?.id ? jidNormalizedUser(socket.user.id) : ''
            for (const message of messages) {
                if (!message.message) continue
                const chat = message.key.remoteJid
                const group = chat?.endsWith('@g.us') ?? false
                const self = me !== '' && chat === me
                // Your own messages count only in the "message yourself" chat.
                if (message.key.fromMe && !self) continue
                const context = message.message?.extendedTextMessage?.contextInfo ?? message.message?.imageMessage?.contextInfo
                    ?? message.message?.videoMessage?.contextInfo ?? message.message?.documentMessage?.contextInfo
                const mentioned = (context?.mentionedJid ?? []).some((jid) => jidNormalizedUser(jid) === me)
                // Answering the bot's own message, or quoting one, is how a group asks it something.
                const quoted = context?.participant ? jidNormalizedUser(context.participant) === me : false
                if (group && !mentioned && !quoted && !self) continue
                const sender = message.key.participant ? jidNormalizedUser(message.key.participant) : (chat ? jidNormalizedUser(chat) : '')
                let media = null
                try {
                    if (mediaOf(message)) {
                        const buffer = await downloadMediaMessage(message, 'buffer', {})
                        media = { ...mediaOf(message), data: buffer.toString('base64') }
                    }
                } catch {}
                out({
                    type: 'message',
                    id: message.key.id,
                    chat,
                    sender,
                    fromMe: message.key.fromMe ?? false,
                    group,
                    mentioned,
                    quoted,
                    name: message.pushName ?? null,
                    text: textOf(message),
                    timestamp: Number(message.messageTimestamp ?? 0) * 1000,
                    media,
                })
            }
        })
    } catch (error) {
        socket = null
        out({ type: 'state', state: 'error', error: String(error?.message ?? error) })
    } finally {
        starting = false
    }
}

async function stop() {
    try {
        socket?.end(undefined)
    } catch {}
    socket = null
    out({ type: 'state', state: 'stopped' })
}

async function logout() {
    try {
        await socket?.logout()
    } catch {}
    socket = null
    out({ type: 'state', state: 'loggedOut' })
}

async function send(command) {
    if (!socket) return out({ type: 'error', id: command.id, error: 'WhatsApp is not connected' })
    try {
        const content = command.path
            ? { document: { url: command.path }, fileName: command.name ?? 'file', mimetype: command.mimetype ?? 'application/octet-stream', caption: command.text ?? undefined }
            : { text: command.text ?? '' }
        const sent = await socket.sendMessage(command.to, content)
        out({ type: 'sent', id: command.id, messageId: sent?.key?.id ?? null })
    } catch (error) {
        out({ type: 'error', id: command.id, error: String(error?.message ?? error) })
    }
}

for await (const line of createInterface({ input: process.stdin })) {
    if (!line.trim()) continue
    let command
    try {
        command = JSON.parse(line)
    } catch {
        continue
    }
    if (command.type === 'start') await start()
    else if (command.type === 'stop') await stop()
    else if (command.type === 'logout') await logout()
    else if (command.type === 'send') await send(command)
    else if (command.type === 'chats') {
        out({ type: 'chats', id: command.id, chats: Object.values(socket?.store?.chats?.all?.() ?? {}).map((c) => ({ id: c.id, name: c.name })) })
    } else if (command.type === 'ping') out({ type: 'pong' })
}

await stop()
process.exit(0)
