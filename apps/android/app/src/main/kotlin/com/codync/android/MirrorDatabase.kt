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
import com.codync.android.core.threadNamed
import java.io.File
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

/**
 * A stored row's front as far as one bounded read takes it ([MirrorDatabase.storedSlice]), and whether that front is
 * the whole row. A sheet shows a slice that is short of its row with [STORED_CUT_MARK]; one that is not short of it is
 * the whole payload, and failing to read it is a failure rather than a cut.
 */
internal class StoredSlice(val payload: String, val complete: Boolean)

/** All entity writes, send reconciliation and the event cursor commit together. */
class MirrorDatabase(context: Context, directory: File, computerId: String) :
    SQLiteOpenHelper(context, file(directory, computerId).absolutePath, null, SCHEMA_VERSION) {
    companion object {
        private const val SCHEMA_VERSION = 6
        /** Hidden rows read per chat row before a resident window stops. Keeps a tool-heavy chat bounded. */
        private const val RAW_PER_SHOWN = 10
        /** Characters of stored payload one chat may hold in memory, so a run of very large rows can't outgrow the heap by itself. */
        private const val RESIDENT_BYTES = 160_000L
        private const val RESIDENT_BYTES_MAX = 480_000L
        private const val BYTES_PER_SHOWN = 2_000L
        /** Characters of stored payload a chat that isn't open may hold as its preview. */
        private const val PREVIEW_BYTES = 8_000L

        /** The first column of a row's [STUB_FLAG_KEYS], right after its length, payload, tail, route and route trust. */
        private const val FLAG_COLUMN = 5

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
    /**
     * The conversation is a cache of the computer's, read only by the schema that wrote it: from any older version it
     * is dropped and the reconnect fetches it again. Bots, unsent messages and the other meta stay; the cursor starts
     * over at 0, which is the same path a cache written before version 3 already took.
     */
    override fun onUpgrade(db: SQLiteDatabase, oldVersion: Int, newVersion: Int) {
        // A cache that lost its meta table must still open: the cursor restart below is the only row written here.
        db.execSQL("CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT NOT NULL)")
        db.execSQL("DROP TABLE IF EXISTS entries")
        db.execSQL("DROP TABLE IF EXISTS history")
        createConversation(db)
        setMeta(db, "rev", "0")
    }
    private fun createConversation(db: SQLiteDatabase) {
        db.execSQL("CREATE TABLE entries(id TEXT PRIMARY KEY, bot_id TEXT NOT NULL, seq INTEGER NOT NULL, rev INTEGER NOT NULL, nonce TEXT, payload TEXT NOT NULL, thread_id TEXT)")
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

    /** One row's original payload. The snapshot does not decode this when the row is over [PER_MESSAGE_CHARS]. */
    @Synchronized internal fun storedPayload(botId: String, id: String): String? = readableDatabase.rawQuery(
        "SELECT payload FROM entries WHERE bot_id=? AND id=? LIMIT 1", arrayOf(botId, id)).use {
        if (it.moveToFirst()) it.getString(0) else null
    }

    /**
     * One row's payload as far as the sheet that opens it reads: at most [PER_MESSAGE_CHARS] characters of it, however
     * long the stored row is, so a message bigger than one cursor window still opens. [StoredSlice.complete] says
     * whether that is the row itself; a slice short of it is closed by the caller (`closedPayloadSlice`). Null when the
     * row is gone. A caller that needs the whole row, such as an approval's answers, reads [storedPayload] instead.
     */
    @Synchronized internal fun storedSlice(botId: String, id: String): StoredSlice? = readableDatabase.rawQuery(
        "SELECT length(payload), substr(payload, 1, $PER_MESSAGE_CHARS) FROM entries WHERE bot_id=? AND id=? LIMIT 1",
        arrayOf(botId, id)).use {
        if (it.moveToFirst()) StoredSlice(it.getString(1).orEmpty(), it.getLong(0) <= PER_MESSAGE_CHARS) else null
    }

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
     * Another computer database starts over. A host whose store schema changed may have rewritten or removed rows
     * in place: the conversation is fetched again from rev 0, and since that catch-up brings only each chat's
     * newest entries and never says one is gone, the held ones go too (unsent messages stay). A new app or host
     * version alone keeps them: rows are stored as the computer sent them, and it gives a changed one a new rev.
     * [stamp] is the host's schema, or its version before it sent one. Returns whether the conversation went.
     */
    @Synchronized fun setStamp(hostId: String, stamp: String, hostVersion: String? = null): Boolean = transaction { db ->
        val otherHost = meta(db, "hostId")?.let { it != hostId } == true
        val held = meta(db, "stamp")
        // Before 2.45.1 the stamp was "<app>/<host version>": the same host version has the same schema.
        val sameHostVersion = held != null && '/' in held && held.substringAfter('/') == hostVersion
        val otherStamp = held != null && held != stamp && !sameHostVersion
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
            // Artifacts, the network, the name and a thread's transcript live in memory (`AppStore.receiveArtifacts`, `receiveNetwork`,
            // `setHostName`, `receiveThreads`).
            MirrorEvent.Resync, MirrorEvent.Ignored, is MirrorEvent.ArtifactChanged, is MirrorEvent.Call, is MirrorEvent.Network,
            is MirrorEvent.Name, is MirrorEvent.AutoOrder, is MirrorEvent.ThreadKeepDays, is MirrorEvent.ThreadEntryChanged -> Unit
            // Workflow events carry no rev. Applying one here would not move the cursor, and must not.
            is MirrorEvent.Workflow -> Unit
            // Model lists live in memory (`AppStore.receiveModels`) and carry no rev.
            is MirrorEvent.Models -> Unit
        }
    }

    // RPC replies and history may arrive ahead of live events; they must not advance the event cursor.
    @Synchronized fun receive(bot: Bot) = transaction { writeBot(it, bot) }
    @Synchronized fun receive(bots: Collection<Bot>) = transaction { db -> bots.forEach { writeBot(db, it) } }
    /** Whether the bots held were read under another app or host version: either may add fields to a bot without a new rev. */
    @Synchronized fun rosterStale(versions: String): Boolean = meta(readableDatabase, "versions") != versions
    /**
     * The current bots, read under [versions]. The event cursor stays: a catch-up from rev 0 brings only each
     * chat's newest entries, and onto the rows held it could leave a gap that paging would never fill.
     */
    @Synchronized fun receiveRoster(bots: Collection<Bot>, versions: String) = transaction { db ->
        bots.forEach { writeBot(db, it) }
        setMeta(db, "versions", versions)
    }
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
            // The row's route is kept beside its payload, so reading a fragment of a large row never loses it.
            put("thread_id", entry.routingThreadId())
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

    /** Resident chats come back with their newest [Residency.limit] chat rows. Every other chat keeps its newest [Residency.preview] rows, while the shared preview budget has room. */
    private fun entries(db: SQLiteDatabase, residency: Residency): List<Entry> {
        val resident = mutableListOf<Entry>()
        val chats = botIds(db)
        for (bot in chats) if (bot in residency.full) resident += residentEntries(db, bot, residency.limit(bot))
        val previewed = previewOrder(residency.previewPriority, chats.filter { it !in residency.full }).let { order ->
            previewsWithin(order, AGGREGATE_PREVIEW_BYTES) { bot -> previewEntries(db, bot, residency.preview) }
        }
        return (resident + previewed).sortedBy { it.seq }
    }

    /** A tool run stops after [RAW_PER_SHOWN] rows per chat row instead of the whole table; the budget grows with the window, up to a fixed ceiling. */
    private fun residentEntries(db: SQLiteDatabase, bot: String, target: Int): List<Entry> {
        val budget = (target * BYTES_PER_SHOWN).coerceIn(RESIDENT_BYTES, RESIDENT_BYTES_MAX)
        return answerable(db, newestPayloads(db, bot, target.toLong() * RAW_PER_SHOWN) { windowRows(it, target, budget) }, budget).map { it.entry }
    }

    /** A chat that isn't open keeps its newest [limit] rows, fewer when they are large. */
    private fun previewEntries(db: SQLiteDatabase, bot: String, limit: Int): List<KeptRow> =
        answerable(db, newestPayloads(db, bot, limit.toLong()) { windowRows(it, limit, PREVIEW_BYTES) }, PREVIEW_BYTES)

    /**
     * [rows] with each approval the computer still waits on read whole, when that row fits in [budget] together with
     * every retained row. An answered or expired card stays the stub [windowRows] already counted. One row at a time,
     * newest first, so the card on screen gets the room. An approval whose row is gone stays a stub; one whose stored
     * row cannot be read says so on its card instead of failing silently.
     */
    private fun answerable(db: SQLiteDatabase, rows: List<KeptRow>, budget: Long): List<KeptRow> {
        val kept = rows.toMutableList()
        var spent = kept.sumOf { it.cost.toLong() }
        for (index in kept.indices.reversed()) {
            val row = kept[index]
            if (!row.entry.isApprovalStub) continue
            // Routing metadata the schema could not trust (too long, or a NUL that hides its real length) keeps the stub:
            // the row is not brought back as an answerable card under a route nothing proved.
            if (row.routingAmbiguous) continue
            // A status the stub already has is enough. Only a card that can still be answered is worth its whole row.
            // No status means the head never finished one, so the row is read and kept only when it is still pending.
            if (row.entry.status != null && row.entry.status != "pending") continue
            // Reading the whole value can fail before decoding (a cursor window too small for the row): that row must not
            // abort the snapshot and hide every other chat either.
            val payload = try {
                storedPayload(row.entry.botId, row.entry.id)
            } catch (error: Exception) {
                android.util.Log.e("Codync", "Couldn't load stored approval ${row.entry.id} in ${row.entry.botId}", error)
                reportReadFailure(row.entry.id, ApprovalReadFailure.Load)
                kept[index] = KeptRow(row.entry.withApprovalReadFailure(), row.cost, row.routingThread)
                continue
            } ?: continue
            val decoded = try {
                WireJson.decodeFromString<Entry>(payload)
            } catch (error: Exception) {
                android.util.Log.e("Codync", "Couldn't read stored approval ${row.entry.id} in ${row.entry.botId}", error)
                reportReadFailure(row.entry.id, ApprovalReadFailure.Decode)
                kept[index] = KeptRow(row.entry.withApprovalReadFailure(), row.cost, row.routingThread)
                continue
            }
            ReadFailureReports.clear(row.entry.id)
            // WireJson takes the last of two duplicate keys, so it can name a thread the strict walk rejected. A row whose
            // decoded route disagrees stays the read-only stub the window built: no answers under an unproven thread, and
            // it appears in the owner chat only. The checked route is what a decoded row keeps everywhere else.
            val named = decoded.threadNamed()
            if (named.routingThreadId() != row.routingThread) continue
            if (named.status != "pending") continue
            val next = spent - row.cost + payload.length
            if (next > budget) continue
            spent = next
            kept[index] = KeptRow(named.withCheckedThread(row.routingThread), payload.length, row.routingThread)
        }
        return kept
    }

    /** One approval-read event per row per failure. [answerable] forgets the row when the payload decodes. */
    private fun reportReadFailure(rowId: String, reason: ApprovalReadFailure) {
        if (ReadFailureReports.once(rowId, reason.wire)) Analytics.capture(AnalyticsEvent.ApprovalReadFailed(reason))
    }

    /**
     * [bot]'s newest [limit] stored payloads, newest first, read while the cursor is open.
     * A payload longer than [PER_MESSAGE_CHARS] comes back as its first [PAYLOAD_HEAD_CHARS] and last [PAYLOAD_TAIL_CHARS],
     * with its full length. Its route comes from the column stored beside it, never from its text, and the boolean
     * control fields a stub keeps are projected from wherever the row holds them ([STUB_FLAG_KEYS]).
     */
    private fun <T> newestPayloads(db: SQLiteDatabase, bot: String, limit: Long, read: (Sequence<StoredRow>) -> T): T =
        db.rawQuery(
            "SELECT length(payload), " +
                "CASE WHEN length(payload) <= $PER_MESSAGE_CHARS THEN payload ELSE substr(payload, 1, $PAYLOAD_HEAD_CHARS) END, " +
                "CASE WHEN length(payload) <= $PER_MESSAGE_CHARS THEN NULL ELSE substr(payload, -$PAYLOAD_TAIL_CHARS) END, " +
                // A routing id is small text, and a slice of a huge value would still fill the cursor window: the
                // projection is bounded whatever the row holds. SQLite's length() counts characters before a NUL, so
                // the byte copy is what proves there is none.
                "CASE WHEN thread_id IS NOT NULL AND length(thread_id) <= $MAX_ROUTING_ID_CHARS " +
                "AND instr(CAST(thread_id AS BLOB), x'00') = 0 THEN substr(thread_id, 1, $MAX_ROUTING_ID_CHARS) ELSE NULL END, " +
                // Metadata this mirror cannot trust: no route, and the card keeps the stub instead of offering answers.
                "CASE WHEN thread_id IS NOT NULL AND (length(thread_id) > $MAX_ROUTING_ID_CHARS " +
                "OR instr(CAST(thread_id AS BLOB), x'00') > 0) THEN 1 ELSE 0 END, " +
                // A stub's kind sits after the body, where a large replyTo or files member written after it leaves it in
                // the unread middle. Each flag is read from where the row holds it, in a bounded slice of it; a row that
                // does not hold the name at all gives a slice that is not the flag, so it adds none.
                STUB_FLAG_KEYS.joinToString(", ") { key ->
                    "substr(payload, max(instr(payload, '\"$key\"'), 1), $FLAG_FRAGMENT_CHARS)"
                } +
                " FROM entries WHERE bot_id=? ORDER BY seq DESC LIMIT $limit",
            arrayOf(bot),
        ).use { cursor ->
            read(generateSequence {
                if (!cursor.moveToNext()) return@generateSequence null
                val chars = cursor.getLong(0)
                val head = cursor.getString(1).orEmpty()
                val tail = if (chars > PER_MESSAGE_CHARS) cursor.getString(2).orEmpty() else null
                val ambiguous = cursor.getInt(4) != 0
                val stored = cursor.getString(3)?.takeIf { it.isNotBlank() && '\u0000' !in it && it.length <= MAX_ROUTING_ID_CHARS }
                val flags = STUB_FLAG_KEYS.indices.mapNotNull { index ->
                    cursor.getString(FLAG_COLUMN + index)?.let(::stubFlag)
                }.toMap()
                // Every row is written with its route beside it, so a missing one is the row's answer, not a gap to fill.
                StoredRow(head, chars, tail, if (ambiguous) null else stored, ambiguous, flags)
            })
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
