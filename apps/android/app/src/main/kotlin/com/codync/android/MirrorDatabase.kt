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
    SQLiteOpenHelper(context, File(directory, "mirror-$computerId.db").absolutePath, null, 3) {
    init { require(computerId.matches(Regex("[A-Za-z0-9_-]{22}"))) }

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

    @Synchronized fun snapshot(): MirrorSnapshot {
        val db = readableDatabase
        return MirrorSnapshot(
            rows(db, "SELECT payload FROM bots WHERE deleted=0") { WireJson.decodeFromString<Bot>(it) },
            rows(db, "SELECT payload FROM entries ORDER BY seq") { WireJson.decodeFromString<Entry>(it) },
            rows(db, "SELECT payload FROM pending ORDER BY created") { WireJson.decodeFromString<PendingSend>(it) },
            meta(db, "rev")?.toLong() ?: 0,
            meta(db, "usage")?.let { WireJson.parseToJsonElement(it).jsonObject } ?: JsonObject(emptyMap()),
            meta(db, "screen")?.let { WireJson.parseToJsonElement(it).jsonObject },
        )
    }

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
            is MirrorEvent.Hello -> {
                if (meta(db, "hostId")?.let { it != event.hostId } == true) reset(db)
                setMeta(db, "hostId", event.hostId)
                if (event.rev < (meta(db, "rev")?.toLong() ?: 0)) reset(db)
                setMeta(db, "usage", event.usage.toString())
                event.screen?.let { setMeta(db, "screen", it.toString()) }
                // Hello's head revision precedes catch-up; it is never a checkpoint.
            }
            is MirrorEvent.BotChanged -> { writeBot(db, event.bot); bump(db, event.bot.rev) }
            is MirrorEvent.EntryChanged -> { writeEntry(db, event.entry); bump(db, event.entry.rev) }
            is MirrorEvent.Usage -> setMeta(db, "usage", event.value.toString())
            is MirrorEvent.Screen -> setMeta(db, "screen", event.value.toString())
            // Artifacts live in memory (`AppStore.receiveArtifacts`).
            MirrorEvent.Resync, MirrorEvent.Ignored, is MirrorEvent.ArtifactChanged -> Unit
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
        rows(db, "SELECT payload FROM pending") { WireJson.decodeFromString<PendingSend>(it) }
            .filter { it.status == SendStatus.Sending }.forEach { writePending(db, it.copy(status = SendStatus.Failed,
                error = "Sending was interrupted. Retry uses the same message ID.")) }
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
            rows(db, "SELECT payload FROM pending") { WireJson.decodeFromString<PendingSend>(it) }
                .filter { it.botId == bot.id }.forEach { db.delete("pending", "nonce=?", arrayOf(it.nonce)) }
            db.delete("history", "lane=?", arrayOf(bot.id))
        }
    }
    private fun writeEntry(db: SQLiteDatabase, entry: Entry) {
        val deleted = db.rawQuery("SELECT deleted FROM bots WHERE id=?", arrayOf(entry.botId)).use { it.moveToFirst() && it.getInt(0) == 1 }
        if (deleted) return
        val revision = db.rawQuery("SELECT rev FROM entries WHERE id=?", arrayOf(entry.id)).use { if (it.moveToFirst()) it.getLong(0) else -1 }
        if (revision > entry.rev) return
        check(db.insertWithOnConflict("entries", null, ContentValues().apply {
            put("id", entry.id); put("bot_id", entry.botId)
            put("seq", entry.seq); put("rev", entry.rev); put("payload", WireJson.encodeToString(entry))
            put("nonce", entry.clientNonce?.takeIf { entry.kind == "user" })
        }, SQLiteDatabase.CONFLICT_REPLACE) != -1L) { "Couldn't save the conversation update." }
        entry.clientNonce?.takeIf { entry.kind == "user" && it.isNotEmpty() }?.let { db.delete("pending", "nonce=?", arrayOf(it)) }
    }
    private fun reset(db: SQLiteDatabase) {
        db.delete("bots", null, null)
        dropConversation(db)
        db.delete("meta", "k IN ('usage','screen')", null)
    }
    private fun dropConversation(db: SQLiteDatabase) {
        listOf("entries", "history").forEach { db.delete(it, null, null) }
        setMeta(db, "rev", "0")
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
    private fun <T> rows(db: SQLiteDatabase, query: String, decode: (String) -> T): List<T> = db.rawQuery(query, null).use {
        buildList { while (it.moveToNext()) add(decode(it.getString(0))) }
    }
    private fun <T> transaction(block: (SQLiteDatabase) -> T): T {
        val db = writableDatabase
        db.beginTransaction()
        try { return block(db).also { db.setTransactionSuccessful() } } finally { db.endTransaction() }
    }
}
