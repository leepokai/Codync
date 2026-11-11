package com.codync.android

import android.content.ContentValues
import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper
import com.codync.android.core.Bot
import com.codync.android.core.Entry
import com.codync.android.core.MirrorEvent
import com.codync.android.core.MirrorSnapshot
import com.codync.android.core.PendingSend
import com.codync.android.core.SendStatus
import com.codync.android.core.WireJson
import java.io.File
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

/** All entity writes, send reconciliation and the event cursor commit together. */
class MirrorDatabase(context: Context, directory: File, computerId: String) :
    SQLiteOpenHelper(context, file(directory, computerId).absolutePath, null, 3) {
    companion object {
        /** Hidden rows read per chat row before a resident window stops. Keeps a tool-heavy chat bounded. */
        private const val RAW_PER_SHOWN = 10
        /** Characters of stored payload one chat may hold in memory, so a run of very large rows can't outgrow the heap by itself. */
        private const val RESIDENT_BYTES = 6_000_000L
        private const val RESIDENT_BYTES_MAX = 24_000_000L
        private const val BYTES_PER_SHOWN = 20_000L
        /** Characters of stored payload a chat that isn't open may hold as its preview. */
        private const val PREVIEW_BYTES = 150_000L

        private fun file(directory: File, computerId: String): File {
            require(computerId.matches(Regex("[A-Za-z0-9_-]{22}")))
            return File(directory, "mirror-$computerId.db")
        }
        /** Deletes a computer's mirror with its journal files, once it's closed. */
        fun delete(directory: File, computerId: String) { SQLiteDatabase.deleteDatabase(file(directory, computerId)) }
        /** A mirror [LocalStore.adopt] copied from account folder [from]: its queued messages name this folder's copies of their files. */
        fun relocate(context: Context, directory: File, computerId: String, from: File) {
            if (file(directory, computerId).exists()) MirrorDatabase(context, directory, computerId).use { it.relocate(from, directory) }
        }
    }

    override fun onCreate(db: SQLiteDatabase) {
        db.execSQL("CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT NOT NULL)")
        db.execSQL("CREATE TABLE bots(id TEXT PRIMARY KEY, rev INTEGER NOT NULL, deleted INTEGER NOT NULL, payload TEXT NOT NULL)")
        createConversation(db)
        db.execSQL("CREATE TABLE pending(nonce TEXT PRIMARY KEY, created INTEGER NOT NULL, payload TEXT NOT NULL)")
    }
    /** The conversation is a cache of the computer's: an older schema's is dropped and fetched again; unsent messages stay. */
    override fun onUpgrade(db: SQLiteDatabase, oldVersion: Int, newVersion: Int) {
        db.execSQL("DROP TABLE IF EXISTS entries")
        db.execSQL("DROP TABLE IF EXISTS history")
        createConversation(db)
        setMeta(db, "rev", "0")
    }
    private fun createConversation(db: SQLiteDatabase) {
        db.execSQL("CREATE TABLE entries(id TEXT PRIMARY KEY, bot_id TEXT NOT NULL, seq INTEGER NOT NULL, rev INTEGER NOT NULL, nonce TEXT, payload TEXT NOT NULL)")
        db.execSQL("CREATE INDEX entries_chat ON entries(bot_id, seq)")
        db.execSQL("CREATE INDEX entries_nonce ON entries(bot_id, nonce)")
        db.execSQL("CREATE TABLE history(lane TEXT PRIMARY KEY)")
    }

    private var decodedIcons: Map<String, com.codync.android.core.CustomIcon>? = null
    private var decodedIconsRevision: String? = null

    private var iconConnectionEpoch = 0L

    /** Per chat, the oldest row the mirror held when this event stream began, set by its hello (iOS `windowFloors`). */
    private var streamFloors: Map<String, Long> = emptyMap()

    /** A reconnect's snapshot is authoritative even after restoring an older database with the same host id. */
    @Synchronized fun beginIconConnection(): Long = transaction { db ->
        iconConnectionEpoch++
        db.delete("meta", "k='iconsRevision'", null)
        iconConnectionEpoch
    }

    @Synchronized fun iconEpoch(): Long = iconConnectionEpoch

    /** A reply from a previous connection cannot repopulate the newly restored cache. */
    @Synchronized fun applyIconReply(event: MirrorEvent.Icons, epoch: Long): Boolean {
        if (epoch != iconConnectionEpoch) return false
        apply(event)
        return true
    }

    /** Read only the image metadata; a push need not load the conversation history. */
    @Synchronized fun icons(): Map<String, com.codync.android.core.CustomIcon> {
        val db = readableDatabase
        val revision = meta(db, "iconsRevision")
        if (decodedIcons != null && decodedIconsRevision == revision) return requireNotNull(decodedIcons)
        return meta(db, "icons")?.let { com.codync.android.core.decodeCustomIcons(WireJson.parseToJsonElement(it)) }.orEmpty().also {
            decodedIcons = it
            decodedIconsRevision = revision
        }
    }

    /**
     * Each resident chat with enough rows to hold its [Residency.limit] chat rows, every other chat with its newest
     * [Residency.preview] rows, so a large roster is not decoded whole.
     */
    @Synchronized fun snapshot(residency: Residency): MirrorSnapshot {
        val db = readableDatabase
        val icons = icons()
        return MirrorSnapshot(
            rows(db, "SELECT payload FROM bots WHERE deleted=0") { WireJson.decodeFromString<Bot>(it) }
                .map { it.copy(customIcon = icons["bot:${it.id}"]) },
            entries(db, residency),
            pending(db, "ORDER BY created"),
            meta(db, "rev")?.toLong() ?: 0,
            meta(db, "usage")?.let { WireJson.parseToJsonElement(it).jsonObject } ?: JsonObject(emptyMap()),
            meta(db, "screen")?.let { WireJson.parseToJsonElement(it).jsonObject },
            icons,
        )
    }

    /** Whether [botId]'s chat already holds the sent message with id [nonce], even outside the rows in memory. */
    @Synchronized internal fun hasNonce(botId: String, nonce: String): Boolean = readableDatabase.rawQuery(
        "SELECT 1 FROM entries WHERE bot_id=? AND nonce=? LIMIT 1", arrayOf(botId, nonce)).use { it.moveToFirst() }

    /**
     * Which drafts' message ids the mirror already holds as a sent message, without decoding payloads, so a draft is not
     * offered twice. A draft's key is its chat's id, or "chat/artifact/id" for one of its artifacts: the message is the chat's.
     */
    @Synchronized internal fun confirmedNonces(drafts: Collection<ComposerDraft>): Set<String> =
        drafts.filter { hasNonce(it.botId.substringBefore('/'), it.nonce) }.mapTo(mutableSetOf()) { it.nonce }

    /** Whether SQLite holds rows of [botId] older than [seq], so paging back can skip the computer. */
    @Synchronized internal fun hasOlder(botId: String, seq: Long): Boolean = readableDatabase.rawQuery(
        "SELECT 1 FROM entries WHERE bot_id=? AND seq<? LIMIT 1", arrayOf(botId, seq.toString())).use { it.moveToFirst() }

    @Synchronized fun revision(): Long = meta(readableDatabase, "rev")?.toLong() ?: 0

    /**
     * Another computer database, app build or host version fetches the conversation again from rev 0, like iOS
     * and desktop: the catch-up brings only each chat's newest entries and never says one is gone, so the held
     * ones go too (unsent messages stay). Returns whether they went.
     */
    @Synchronized fun setStamp(hostId: String, stamp: String): Boolean = transaction { db ->
        val otherHost = meta(db, "hostId")?.let { it != hostId } == true
        val otherStamp = meta(db, "stamp")?.let { it != stamp } == true
        if (otherHost) reset(db) else if (otherStamp) dropConversation(db)
        setMeta(db, "hostId", hostId)
        setMeta(db, "stamp", stamp)
        otherHost || otherStamp
    }

    @Synchronized fun apply(event: MirrorEvent) = apply(listOf(event))
    @Synchronized fun apply(events: List<MirrorEvent>) = transaction { db ->
        for (event in events) when (event) {
            is MirrorEvent.Icons -> if (event.revision >= (meta(db, "iconsRevision")?.toLong() ?: -1)) {
                setMeta(db, "icons", WireJson.encodeToString(event.icons))
                setMeta(db, "iconsRevision", event.revision.toString())
                decodedIcons = null
            }
            is MirrorEvent.Hello -> {
                if (meta(db, "hostId")?.let { it != event.hostId } == true) reset(db)
                setMeta(db, "hostId", event.hostId)
                if (event.rev < (meta(db, "rev")?.toLong() ?: 0)) reset(db)
                streamFloors = floorsAtCursor(db)
                setMeta(db, "usage", event.usage.toString())
                event.screen?.let { setMeta(db, "screen", it.toString()) }
                // Hello's head revision precedes catch-up; it is never a checkpoint.
            }
            is MirrorEvent.BotChanged -> { writeBot(db, event.bot); bump(db, event.bot.rev) }
            is MirrorEvent.EntryChanged -> { writeEntry(db, event.entry, live = true); bump(db, event.entry.rev) }
            is MirrorEvent.Usage -> setMeta(db, "usage", event.value.toString())
            is MirrorEvent.Screen -> setMeta(db, "screen", event.value.toString())
            // Artifacts, the network and the name live in memory (`AppStore.receiveArtifacts`, `receiveNetwork`, `setHostName`).
            MirrorEvent.Resync, MirrorEvent.Ignored, is MirrorEvent.ArtifactChanged, is MirrorEvent.Call, is MirrorEvent.Network,
            is MirrorEvent.Name -> Unit
        }
    }

    // RPC replies and history may arrive ahead of live events; they must not advance the event cursor.
    @Synchronized fun receive(bot: Bot) = transaction { writeBot(it, bot) }
    @Synchronized fun receive(bots: Collection<Bot>) = transaction { db -> bots.forEach { writeBot(db, it) } }
    @Synchronized fun receive(entries: List<Entry>) = transaction { db -> entries.forEach { writeEntry(db, it) } }
    @Synchronized fun rewind() = transaction { setMeta(it, "rev", "0") }
    @Synchronized fun save(send: PendingSend) = transaction { db ->
        writePending(db, send)
    }
    private fun writePending(db: SQLiteDatabase, send: PendingSend) {
        val confirmed = db.rawQuery("SELECT 1 FROM entries WHERE bot_id=? AND nonce=?", arrayOf(send.botId, send.nonce)).use { it.moveToFirst() }
        if (confirmed) return
        val count = db.compileStatement("SELECT COUNT(*) FROM pending").use { it.simpleQueryForLong() }
        val exists = db.rawQuery("SELECT 1 FROM pending WHERE nonce=?", arrayOf(send.nonce)).use { it.moveToFirst() }
        require(exists || count < 200) { "Too many pending messages. Retry or discard older messages first." }
        check(db.insertWithOnConflict("pending", null, ContentValues().apply {
            put("nonce", send.nonce); put("created", send.createdAt); put("payload", WireJson.encodeToString(send))
        }, SQLiteDatabase.CONFLICT_REPLACE) != -1L) { "Couldn't save the pending message." }
    }
    @Synchronized fun discard(nonce: String) = transaction { it.delete("pending", "nonce=?", arrayOf(nonce)) }
    @Synchronized fun recoverInterrupted() = transaction { db ->
        pending(db).filter { it.status == SendStatus.Sending }.forEach { writePending(db, it.copy(status = SendStatus.Failed,
            error = "Sending was interrupted. Retry uses the same message ID.")) }
    }
    @Synchronized private fun relocate(from: File, to: File) = transaction { db ->
        pending(db).forEach { writePending(db, it.copy(files = it.files.map { file -> AttachmentFiles.moved(file, from, to) })) }
    }
    @Synchronized fun historyComplete(botId: String): Boolean = readableDatabase.rawQuery(
        "SELECT 1 FROM history WHERE lane=?", arrayOf(botId)).use { it.moveToFirst() }
    @Synchronized fun finishHistory(botId: String) = transaction { db ->
        db.insertWithOnConflict("history", null, ContentValues().apply { put("lane", botId) }, SQLiteDatabase.CONFLICT_IGNORE)
    }

    private fun writeBot(db: SQLiteDatabase, bot: Bot) {
        val revision = db.rawQuery("SELECT rev FROM bots WHERE id=?", arrayOf(bot.id)).use { if (it.moveToFirst()) it.getLong(0) else -1 }
        if (revision > bot.rev) return
        check(db.insertWithOnConflict("bots", null, ContentValues().apply {
            put("id", bot.id); put("rev", bot.rev); put("deleted", if (bot.deleted) 1 else 0); put("payload", WireJson.encodeToString(bot))
        }, SQLiteDatabase.CONFLICT_REPLACE) != -1L) { "Couldn't save the bot update." }
        if (bot.deleted) {
            db.delete("entries", "bot_id=?", arrayOf(bot.id))
            pending(db).filter { it.botId == bot.id }.forEach { db.delete("pending", "nonce=?", arrayOf(it.nonce)) }
            db.delete("history", "lane=?", arrayOf(bot.id))
        }
    }
    /**
     * [live] is an event, not a history page. An old row the computer rewrote, one this phone never held, arrives below
     * the oldest row its chat held when the stream began ([streamFloors]): keeping it would hide the gap before it from paging
     * back, so it waits to be paged in, as on the iPhone.
     */
    private fun writeEntry(db: SQLiteDatabase, entry: Entry, live: Boolean = false) {
        val deleted = db.rawQuery("SELECT deleted FROM bots WHERE id=?", arrayOf(entry.botId)).use { it.moveToFirst() && it.getInt(0) == 1 }
        if (deleted) return
        val revision = db.rawQuery("SELECT rev FROM entries WHERE id=?", arrayOf(entry.id)).use { if (it.moveToFirst()) it.getLong(0) else -1 }
        if (revision > entry.rev) return
        if (live && revision < 0 && streamFloors[entry.botId]?.let { entry.seq < it } == true) return
        check(db.insertWithOnConflict("entries", null, ContentValues().apply {
            put("id", entry.id); put("bot_id", entry.botId)
            put("seq", entry.seq); put("rev", entry.rev); put("payload", WireJson.encodeToString(entry))
            put("nonce", entry.clientNonce?.takeIf { entry.kind == "user" })
        }, SQLiteDatabase.CONFLICT_REPLACE) != -1L) { "Couldn't save the conversation update." }
        entry.clientNonce?.takeIf { entry.kind == "user" && it.isNotEmpty() }?.let { db.delete("pending", "nonce=?", arrayOf(it)) }
    }
    private fun reset(db: SQLiteDatabase) {
        decodedIcons = null
        decodedIconsRevision = null
        db.delete("bots", null, null)
        dropConversation(db)
        db.delete("meta", "k IN ('usage','screen','icons','iconsRevision')", null)
    }
    private fun dropConversation(db: SQLiteDatabase) {
        listOf("entries", "history").forEach { db.delete(it, null, null) }
        setMeta(db, "rev", "0")
        streamFloors = emptyMap()
    }
    /**
     * Each chat's oldest row among those the mirror held at the event cursor. A send's reply stored since comes again in
     * the catch-up and says nothing about the window; a stream from rev 0 rebuilds every chat in rev order, so it has none.
     */
    private fun floorsAtCursor(db: SQLiteDatabase): Map<String, Long> {
        val cursor = meta(db, "rev")?.toLong() ?: 0
        if (cursor == 0L) return emptyMap()
        return db.rawQuery("SELECT bot_id, MIN(seq) FROM entries WHERE rev<=? GROUP BY bot_id", arrayOf(cursor.toString())).use {
            buildMap { while (it.moveToNext()) put(it.getString(0), it.getLong(1)) }
        }
    }
    private fun bump(db: SQLiteDatabase, rev: Long) = setMeta(db, "rev", maxOf(rev, meta(db, "rev")?.toLong() ?: 0).toString())
    private fun meta(db: SQLiteDatabase, key: String): String? = db.rawQuery("SELECT v FROM meta WHERE k=?", arrayOf(key)).use {
        if (it.moveToFirst()) it.getString(0) else null
    }
    private fun setMeta(db: SQLiteDatabase, key: String, value: String) {
        check(db.insertWithOnConflict("meta", null, ContentValues().apply { put("k", key); put("v", value) }, SQLiteDatabase.CONFLICT_REPLACE) != -1L) {
            "Couldn't save the conversation checkpoint."
        }
    }
    private fun pending(db: SQLiteDatabase, order: String = "") =
        rows(db, "SELECT payload FROM pending $order") { WireJson.decodeFromString<PendingSend>(it) }

    /** Resident chats come back with their newest [Residency.limit] chat rows. Every other chat keeps its newest [Residency.preview] rows. */
    private fun entries(db: SQLiteDatabase, residency: Residency): List<Entry> = botIds(db).flatMap { bot ->
        if (bot in residency.full) residentEntries(db, bot, residency.limit(bot)) else previewEntries(db, bot, residency.preview)
    }.sortedBy { it.seq }

    /** A tool run stops after [RAW_PER_SHOWN] rows per chat row instead of the whole table; the budget grows with the window, up to a fixed ceiling. */
    private fun residentEntries(db: SQLiteDatabase, bot: String, target: Int): List<Entry> {
        val budget = (target * BYTES_PER_SHOWN).coerceIn(RESIDENT_BYTES, RESIDENT_BYTES_MAX)
        return newestPayloads(db, bot, target.toLong() * RAW_PER_SHOWN) { newestWindow(it, target, budget) }
    }

    /** A chat that isn't open keeps its newest [limit] rows, fewer when they are large. */
    private fun previewEntries(db: SQLiteDatabase, bot: String, limit: Int): List<Entry> =
        newestPayloads(db, bot, limit.toLong()) { newestWindow(it, limit, PREVIEW_BYTES) }

    /** [bot]'s newest [limit] stored payloads, newest first, read while the cursor is open. */
    private fun <T> newestPayloads(db: SQLiteDatabase, bot: String, limit: Long, read: (Sequence<String>) -> T): T =
        db.rawQuery("SELECT payload FROM entries WHERE bot_id=? ORDER BY seq DESC LIMIT $limit", arrayOf(bot)).use { cursor ->
            read(generateSequence { if (cursor.moveToNext()) cursor.getString(0) else null })
        }

    private fun botIds(db: SQLiteDatabase): List<String> = db.rawQuery("SELECT DISTINCT bot_id FROM entries", null).use { cursor ->
        buildList { while (cursor.moveToNext()) add(cursor.getString(0)) }
    }

    private fun <T> rows(db: SQLiteDatabase, query: String, decode: (String) -> T): List<T> = db.rawQuery(query, null).use {
        buildList { while (it.moveToNext()) add(decode(it.getString(0))) }
    }
    private fun <T> transaction(block: (SQLiteDatabase) -> T): T {
        val db = writableDatabase
        db.beginTransaction()
        try { return block(db).also { db.setTransactionSuccessful() } } finally { db.endTransaction() }
    }
}
