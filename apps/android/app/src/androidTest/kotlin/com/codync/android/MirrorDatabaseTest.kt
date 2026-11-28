package com.codync.android

import androidx.test.platform.app.InstrumentationRegistry
import android.database.sqlite.SQLiteDatabase
import android.content.ContentValues
import com.codync.android.core.*
import java.io.File
import java.util.UUID
import kotlinx.serialization.json.*
import kotlinx.serialization.encodeToString
import org.junit.After
import org.junit.Assert.*
import org.junit.Test

class MirrorDatabaseTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val directory = File(context.noBackupFilesDir, "mirror-test-" + UUID.randomUUID()).apply { mkdirs() }
    private val computer = "AAAAAAAAAAAAAAAAAAAAAA"
    private var mirror = MirrorDatabase(context, directory, computer)
    private fun entry(rev: Long, text: String = "new", nonce: String? = null): Entry = Entry("message", 1, "bot", rev = rev, kind = "user",
        data = buildJsonObject { put("text", text); nonce?.let { put("clientNonce", it) } })

    /**
     * Replaces the mirror with one an older app left, written with that version's columns, and reopens it so the
     * upgrade runs. Version 4 added `thread_id`, version 5 `routing_known`; version 6 keeps only `thread_id` and drops
     * the conversation, so no stored row is ever read by a schema other than the one that wrote it.
     */
    private fun storeOldMirror(
        version: Int,
        rows: List<Pair<Entry, String>> = emptyList(),
        pending: List<PendingSend> = emptyList(),
        bots: List<Bot> = emptyList(),
        rev: Long = 7,
        withMeta: Boolean = true,
        withConversation: Boolean = true,
    ) {
        mirror.close()
        val path = File(directory, "mirror-$computer.db")
        path.delete()
        SQLiteDatabase.openOrCreateDatabase(path, null).use { db ->
            if (withMeta) db.execSQL("CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT NOT NULL)")
            db.execSQL("CREATE TABLE bots(id TEXT PRIMARY KEY, rev INTEGER NOT NULL, deleted INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE pending(nonce TEXT PRIMARY KEY, created INTEGER NOT NULL, payload TEXT NOT NULL)")
            if (withConversation) {
                val route = if (version >= 5) "thread_id TEXT, routing_known INTEGER NOT NULL DEFAULT 0"
                    else if (version >= 4) "thread_id TEXT" else null
                db.execSQL("CREATE TABLE entries(id TEXT PRIMARY KEY, bot_id TEXT NOT NULL, seq INTEGER NOT NULL, rev INTEGER NOT NULL, nonce TEXT, payload TEXT NOT NULL" +
                    (route?.let { ", $it" } ?: "") + ")")
                db.execSQL("CREATE TABLE history(lane TEXT PRIMARY KEY)")
                db.insertOrThrow("history", null, ContentValues().apply { put("lane", "bot") })
                rows.forEach { (row, payload) ->
                    db.insertOrThrow("entries", null, ContentValues().apply {
                        put("id", row.id); put("bot_id", row.botId); put("seq", row.seq); put("rev", row.rev)
                        put("payload", payload)
                        if (version >= 4) put("thread_id", row.routingThreadId())
                    })
                }
            }
            bots.forEach {
                db.insertOrThrow("bots", null, ContentValues().apply {
                    put("id", it.id); put("rev", it.rev); put("deleted", if (it.deleted) 1 else 0)
                    put("payload", WireJson.encodeToString(it))
                })
            }
            pending.forEach {
                db.insertOrThrow("pending", null, ContentValues().apply {
                    put("nonce", it.nonce); put("created", it.createdAt); put("payload", WireJson.encodeToString(it))
                })
            }
            if (withMeta) db.insertOrThrow("meta", null, ContentValues().apply { put("k", "rev"); put("v", rev.toString()) })
            db.version = version
        }
        mirror = MirrorDatabase(context, directory, computer)
    }

    /** Stores one row the app could not have written, with [payload] exactly and no route beside it. */
    private fun storeRaw(approval: Entry, payload: String, neighbor: Entry = chat("bot", 2), thread: String? = null) {
        mirror.receive(listOf(neighbor))
        mirror.close()
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.insertOrThrow("entries", null, ContentValues().apply {
                put("id", approval.id); put("bot_id", approval.botId); put("seq", approval.seq); put("rev", approval.rev)
                put("payload", payload); put("thread_id", thread)
            })
        }
        mirror = MirrorDatabase(context, directory, computer)
    }

    @After fun cleanup() { mirror.close(); directory.deleteRecursively() }

    @Test fun iconCacheTracksNewerRevisionsAndResetsWithTheHostDatabase() {
        mirror.setStamp("old", "build")
        val first = CustomIcon("first")
        val latest = CustomIcon("latest", true)
        mirror.apply(MirrorEvent.Icons(mapOf("bot:b" to first), 1))
        val cached = mirror.icons()
        assertSame(cached, mirror.icons())
        MirrorDatabase(context, directory, computer).use { other ->
            other.apply(MirrorEvent.Icons(mapOf("bot:b" to latest), 2))
        }
        assertEquals(latest, mirror.icons()["bot:b"])
        mirror.apply(MirrorEvent.Icons(mapOf("bot:b" to first), 1))
        assertEquals(latest, mirror.icons()["bot:b"])
        mirror.apply(MirrorEvent.Icons(emptyMap(), 3))
        assertTrue(mirror.icons().isEmpty())
        mirror.setStamp("new", "build")
        mirror.apply(MirrorEvent.Icons(mapOf("bot:b" to first), 0))
        assertEquals(first, mirror.icons()["bot:b"])
    }

    @Test fun reconnectAcceptsSameHostRollbackAboveBotCursorAndRejectsOldSaveReplies() {
        mirror.setStamp("same-host", "build")
        mirror.apply(MirrorEvent.BotChanged(Bot("b", "Bot", rev = 50)))
        val old = CustomIcon("old")
        mirror.apply(MirrorEvent.Icons(mapOf("bot:b" to old), 100))
        val oldEpoch = mirror.iconEpoch()
        val currentEpoch = mirror.beginIconConnection()
        assertEquals(old, mirror.icons()["bot:b"]) // Keep the offline picture until catch-up arrives.
        mirror.apply(MirrorEvent.Hello("same-host", 90, JsonObject(emptyMap()), null))
        val restored = CustomIcon("restored")
        mirror.apply(MirrorEvent.Icons(mapOf("bot:b" to restored), 90))
        assertEquals(50, mirror.revision())
        assertEquals(restored, mirror.icons()["bot:b"])
        assertFalse(mirror.applyIconReply(MirrorEvent.Icons(mapOf("bot:b" to old), 110), oldEpoch))
        assertEquals(restored, mirror.icons()["bot:b"])
        assertTrue(mirror.applyIconReply(MirrorEvent.Icons(emptyMap(), 91), currentEpoch))
        assertTrue(mirror.icons().isEmpty())
    }

    @Test fun rpcRepliesAndHelloCannotCheckpointPastUnseenEvents() {
        mirror.apply(MirrorEvent.Hello("database", 100, JsonObject(emptyMap()), null))
        mirror.receive(listOf(entry(90)))
        assertEquals(0, mirror.snapshot().rev)
        mirror.apply(MirrorEvent.EntryChanged(entry(2, "older")))
        assertEquals(2, mirror.snapshot().rev)
        assertEquals("new", mirror.snapshot().entries.single().text)
    }

    @Test fun anInitialRosterCannotSkipEarlierTranscriptEventsAndBatchesCommitTogether() {
        mirror.receive(listOf(Bot("bot", "Current roster", rev = 500)))
        assertEquals(0, mirror.snapshot().rev)
        mirror.save(PendingSend("nonce", "bot", text = "message", createdAt = 1))
        mirror.apply(listOf(MirrorEvent.BotChanged(Bot("bot", "Older roster", rev = 2)),
            MirrorEvent.EntryChanged(entry(3, nonce = "nonce"))))
        mirror.close()
        mirror = MirrorDatabase(context, directory, computer)
        val snapshot = mirror.snapshot()
        assertEquals(3, snapshot.rev)
        assertEquals("Current roster", snapshot.bots.single().name)
        assertEquals("nonce", snapshot.entries.single().clientNonce)
        assertTrue(snapshot.pending.isEmpty())
    }

    @Test fun cursorAndSendReconciliationSurviveReopeningTogether() {
        mirror.save(PendingSend("nonce", "bot", text = "message", createdAt = 1))
        mirror.apply(MirrorEvent.EntryChanged(entry(7, nonce = "nonce")))
        mirror.close()
        mirror = MirrorDatabase(context, directory, computer)
        assertEquals(7, mirror.snapshot().rev)
        assertTrue(mirror.snapshot().pending.isEmpty())
        assertEquals("nonce", mirror.snapshot().entries.single().clientNonce)
    }

    @Test fun tombstonesCannotBeResurrectedByLateRepliesOrHistory() {
        mirror.apply(MirrorEvent.BotChanged(Bot("bot", "Bot", rev = 2)))
        mirror.apply(MirrorEvent.EntryChanged(entry(3)))
        mirror.apply(MirrorEvent.BotChanged(Bot("bot", rev = 4, deleted = true)))
        mirror.receive(Bot("bot", "Old", rev = 2))
        mirror.receive(listOf(entry(3)))
        assertTrue(mirror.snapshot().bots.isEmpty())
        assertTrue(mirror.snapshot().entries.isEmpty())
        assertEquals(4, mirror.snapshot().rev)
    }

    @Test fun incompatibleCacheAndDatabaseReplacementRewindSafely() {
        assertFalse(mirror.setStamp("old", "schema 7"))
        mirror.apply(MirrorEvent.EntryChanged(entry(9)))
        mirror.save(PendingSend("unsent", "bot", text = "message", createdAt = 1))
        mirror.finishHistory("bot")
        assertFalse(mirror.setStamp("old", "schema 7"))
        assertEquals(1, mirror.snapshot().entries.size)
        // The new host removed "message": the catch-up from 0 brings only what it still has.
        assertTrue(mirror.setStamp("old", "schema 8"))
        assertEquals(0, mirror.snapshot().rev)
        assertFalse(mirror.historyComplete("bot"))
        assertEquals("unsent", mirror.snapshot().pending.single().nonce)
        mirror.apply(MirrorEvent.EntryChanged(Entry("kept", 2, "bot", rev = 10, kind = "user", data = buildJsonObject { put("text", "kept") })))
        assertEquals(listOf("kept"), mirror.snapshot().entries.map { it.id })
        assertTrue(mirror.setStamp("new", "schema 8"))
        assertEquals(0, mirror.snapshot().rev)
        assertTrue(mirror.snapshot().entries.isEmpty())
    }

    @Test fun aNewVersionReadsTheBotsAgainAndKeepsTheConversation() {
        mirror.setStamp("host", "schema 8")
        mirror.apply(MirrorEvent.EntryChanged(entry(9)))
        assertTrue(mirror.rosterStale("2.4/2.4"))
        mirror.receiveRoster(listOf(Bot("bot", "Bot", rev = 2)), "2.4/2.4")
        assertFalse(mirror.rosterStale("2.4/2.4"))
        // The cursor stays: a catch-up from the start onto the rows held could leave a gap.
        assertEquals(9, mirror.revision())
        assertTrue(mirror.rosterStale("2.5/2.4"))
        assertFalse(mirror.setStamp("host", "schema 8"))
        assertEquals(1, mirror.snapshot().entries.size)
    }

    @Test fun aStampFromBeforeTheSchemaKeepsTheConversationOnTheSameHostVersion() {
        mirror.setStamp("host", "2.45.0/2.45.0")
        mirror.apply(MirrorEvent.EntryChanged(entry(9)))
        assertFalse(mirror.setStamp("host", "schema 8", hostVersion = "2.45.0"))
        assertEquals(1, mirror.snapshot().entries.size)
        // Stamped with the schema from here on, a new host version alone keeps it too.
        assertFalse(mirror.setStamp("host", "schema 8", hostVersion = "2.45.1"))
        // Synced with another host version, whose schema is unknown.
        mirror.setStamp("host", "2.45.0/2.44.0")
        mirror.apply(MirrorEvent.EntryChanged(entry(9)))
        assertTrue(mirror.setStamp("host", "schema 8", hostVersion = "2.45.0"))
        assertTrue(mirror.snapshot().entries.isEmpty())
    }

    @Test fun interruptedSendRetainsItsNonceAndRequiresAnExplicitRetry() {
        mirror.save(PendingSend("stable", "bot", text = "message", createdAt = 1))
        mirror.close()
        mirror = MirrorDatabase(context, directory, computer)
        mirror.recoverInterrupted()
        assertEquals("stable", mirror.snapshot().pending.single().nonce)
        assertEquals(SendStatus.Failed, mirror.snapshot().pending.single().status)
    }

    @Test fun aLateSendFailureCannotResurrectAnAlreadyConfirmedMessage() {
        val send = PendingSend("stable", "bot", text = "message", createdAt = 1)
        mirror.save(send)
        mirror.apply(MirrorEvent.EntryChanged(entry(3, nonce = send.nonce)))
        mirror.save(send.copy(status = SendStatus.Failed))
        assertTrue(mirror.snapshot().pending.isEmpty())
        assertEquals(1, mirror.snapshot().entries.size)
    }

    @Test fun boundedSnapshotKeepsResidentChatsWholeAndPreviewsTheRest() {
        val keep = (1L..40L).map { chat("keep", it) }
        val previewed = (1L..40L).map { chat("drop", it) }
        val short = (1L..2L).map { chat("short", it) }
        mirror.receive(keep + previewed + short)
        mirror.save(PendingSend("unsent", "drop", text = "waiting", createdAt = 1))
        val everything = mirror.snapshot()
        val bounded = mirror.snapshot(Residency(full = setOf("keep"), preview = 30, window = 40))
        assertEquals((1L..40L).toList(), bounded.entries.filter { it.botId == "keep" }.map { it.seq })
        assertEquals((11L..40L).toList(), bounded.entries.filter { it.botId == "drop" }.map { it.seq })
        assertEquals(listOf(1L, 2L), bounded.entries.filter { it.botId == "short" }.map { it.seq })
        assertFalse(bounded.entries.any { it.clientNonce == "n-drop-1" })
        assertEquals(everything.rev, bounded.rev)
        assertEquals(listOf("unsent"), bounded.pending.map { it.nonce })
        assertEquals(setOf("n-drop-1"), mirror.confirmedNonces(listOf(ComposerDraft("drop", nonce = "n-drop-1"), ComposerDraft("drop", nonce = "never-sent"))))
        assertTrue(mirror.hasNonce("drop", "n-drop-1"))
        assertFalse(mirror.hasNonce("keep", "n-drop-1"))
        assertTrue(mirror.snapshot(Residency(emptySet(), preview = 0)).entries.isEmpty())
    }

    @Test fun aResidentChatLoadsOnlyItsWindowAndOlderRowsStayReachable() {
        mirror.receive((1L..50L).map { chat("big", it) })
        val windowed = mirror.snapshot(Residency(setOf("big"), preview = 5, window = 20))
        assertEquals((31L..50L).toList(), windowed.entries.map { it.seq })
        assertTrue(mirror.hasOlder("big", 31))
        assertFalse(mirror.hasOlder("big", 1))
        val grown = mirror.snapshot(Residency(setOf("big"), preview = 5, window = 20, windows = mapOf("big" to 45)))
        assertEquals((6L..50L).toList(), grown.entries.map { it.seq })
    }

    @Test fun anOldRowTheComputerRewritesDoesNotHideTheGapBeforeIt() {
        mirror.apply((100L..120L).map { MirrorEvent.EntryChanged(chat("big", it)) })
        // A send's reply stored after the cursor comes again in the catch-up: it sets no floor.
        mirror.receive(listOf(chat("quiet", 30).copy(rev = 128)))
        mirror.apply(MirrorEvent.Hello("host", 130, JsonObject(emptyMap()), null))
        mirror.apply(MirrorEvent.EntryChanged(chat("big", 5).copy(rev = 125)))
        assertFalse("a row below the oldest stored one waits to be paged in", mirror.hasNonce("big", "n-big-5"))
        assertEquals(100L, mirror.snapshot(Residency(setOf("big"), window = 50)).entries.filter { it.botId == "big" }.minOf { it.seq })
        mirror.apply(MirrorEvent.EntryChanged(chat("big", 121)))
        assertTrue(mirror.hasNonce("big", "n-big-121"))
        mirror.receive(listOf(chat("big", 5)))
        assertTrue("a history page may reach below the stored rows", mirror.hasNonce("big", "n-big-5"))
        mirror.apply(MirrorEvent.EntryChanged(chat("quiet", 25).copy(rev = 126)))
        assertTrue("a chat with no rows at the cursor takes whatever arrives", mirror.hasNonce("quiet", "n-quiet-25"))
    }

    @Test fun aCatchUpFromTheStartKeepsEveryRowWhateverItsOrder() {
        mirror.setStamp("host", "schema 7")
        mirror.apply((10L..20L).map { MirrorEvent.EntryChanged(chat("bot", it)) })
        // A host with another store schema fetches the conversation again from rev 0, and a send's reply lands before the catch-up.
        assertTrue(mirror.setStamp("host", "schema 8"))
        mirror.receive(listOf(chat("bot", 30)))
        mirror.apply(MirrorEvent.Hello("host", 40, JsonObject(emptyMap()), null))
        // Sorted by rev, so an older row edited later arrives after a newer one.
        mirror.apply(listOf(4L, 2L, 7L).mapIndexed { i, seq -> MirrorEvent.EntryChanged(chat("bot", seq).copy(rev = 31L + i)) })
        assertEquals(listOf(2L, 4L, 7L, 30L), mirror.snapshot().entries.map { it.seq })
    }

    @Test fun anArtifactDraftIsConfirmedByItsChatsMessage() {
        mirror.receive(listOf(chat("bot", 1)))
        val artifact = ComposerDraft("bot/artifact/a1", nonce = "n-bot-1")
        val unsent = ComposerDraft("bot/artifact/a2", nonce = "never")
        assertEquals(setOf("n-bot-1"), mirror.confirmedNonces(listOf(artifact, unsent)))
    }

    @Test fun largeRowsStopAChatFromGrowingPastItsByteBudgetAndAPreviewKeepsItsNewestRow() {
        val filler = "x".repeat(60_000)
        fun big(seq: Long) = Entry("e-big-$seq", seq, "big", rev = seq, kind = "user",
            data = buildJsonObject { put("text", filler); put("clientNonce", "n-big-$seq") })
        mirror.receive((1L..600L).map(::big))
        val small = mirror.snapshot(Residency(setOf("big"), window = 300)).entries.filter { it.botId == "big" }
        val wide = mirror.snapshot(Residency(setOf("big"), window = 500, windows = mapOf("big" to 500))).entries.filter { it.botId == "big" }
        assertEquals("the row cap still applies when each large row is only a stub", 300, small.size)
        assertEquals(500, wide.size)
        assertEquals(600L, small.maxOf { it.seq })
        assertEquals(600L, wide.maxOf { it.seq })
        assertTrue(small.sumOf { it.text.length } < 80_000)
        assertTrue(wide.sumOf { it.text.length } < 80_000)
        assertTrue(small.all { filler !in it.text && it.text == STORED_STUB_ID && storedChars(it) > 60_000 })
        assertTrue(wide.all { filler !in it.text && it.text == STORED_STUB_ID && storedChars(it) > 60_000 })
        val preview = mirror.snapshot(Residency(emptySet(), window = 300, preview = 30)).entries.filter { it.botId == "big" }
        assertEquals("a preview keeps the newest stubs and not the 60k filler", 30, preview.size)
        assertEquals(600L, preview.maxOf { it.seq })
        assertTrue(preview.all { filler !in it.text && it.text == STORED_STUB_ID && storedChars(it) > 60_000 })
    }

    private fun storedChars(entry: Entry) = (entry.data["storedChars"] as? JsonPrimitive)?.longOrNull ?: 0L

    @Test fun anOversizedRowKeepsItsTimeSoItStaysWhereItWasWritten() {
        val filler = "F".repeat(PER_MESSAGE_CHARS + 1)
        val big = Entry("big", 1, "bot", rev = 1, kind = "user", createdAt = 1_700_000_000_000, updatedAt = 1_700_000_001_000,
            data = buildJsonObject { put("text", filler) })
        mirror.receive(listOf(big))
        val kept = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries.single()
        assertTrue(kept.isStoredStub)
        assertEquals(1_700_000_000_000, kept.createdAt)
        assertEquals(1_700_000_001_000, kept.updatedAt)
    }

    @Test fun aThreadHeldBotCanPageBackToAnApprovalMoreThanEightyRowsBehind() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
                add(buildJsonObject { put("optionId", "deny"); put("name", "Deny"); put("kind", "reject_once") })
            })
        })
        mirror.receive(listOf(approval) + (2L..101L).map { chat("bot", it) })
        val preview = mirror.snapshot(Residency(emptySet(), preview = 8, window = 8))
        assertFalse("the thread owner's old card is past the preview", preview.entries.any { it.id == "ask" })
        val first = mirror.snapshot(Residency(setOf("bot"), preview = 8, window = 80))
        assertFalse("the approval is older than the held owner's first window", first.entries.any { it.id == "ask" })
        val residents = ResidentBots()
        val threads = ThreadViews(residents)
        assertTrue(threads.watch("bot", "thread"))
        // The first thread page widens the owner's window (AppStoreChat.pageHistory).
        val widened = mirror.snapshot(Residency(residents.full(null), preview = 8, window = 80,
            windows = mapOf("bot" to Residency(setOf("bot")).widened("bot"))))
        val card = widened.entries.single { it.id == "ask" }
        assertEquals("pending", card.status)
        assertEquals(listOf("allow", "deny"), (card.data["options"] as JsonArray).map { (it as JsonObject).text("optionId") })
        assertTrue(threads.close("bot", "thread"))
        assertFalse("the widened owner window is reclaimed with the thread", mirror.snapshot(Residency(residents.full(null),
            preview = 8, window = 80)).entries.any { it.id == "ask" })
    }

    @Test fun anUnreadableApprovalSaysSoInsteadOfStayingASilentStub() {
        val filler = "D".repeat(PER_MESSAGE_CHARS + 1)
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", filler)
        })
        val payload = WireJson.encodeToString(approval)
        assertTrue(payload.length > PER_MESSAGE_CHARS)
        mirror.receive(listOf(approval))
        mirror.close()
        // A damaged stored row: its head still names an approval, but the whole payload no longer decodes.
        val broken = payload.dropLast(1)
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.execSQL("UPDATE entries SET payload=? WHERE id=?", arrayOf(broken, approval.id))
        }
        mirror = MirrorDatabase(context, directory, computer)
        val kept = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries.single { it.id == "ask" }
        assertTrue("the card still shows where its row is", kept.isStoredStub)
        assertTrue("its read failure is visible to the card", kept.approvalReadFailed)
        assertEquals(storedStubText(broken.length.toLong()), kept.text)
    }

    @Test fun aMalformedShortApprovalDoesNotAbortTheSnapshot() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
        })
        val neighbor = chat("bot", 2)
        mirror.receive(listOf(approval, neighbor))
        mirror.close()
        val broken = WireJson.encodeToString(approval).dropLast(1)
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.execSQL("UPDATE entries SET payload=? WHERE id=?", arrayOf(broken, approval.id))
        }
        mirror = MirrorDatabase(context, directory, computer)
        val kept = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = kept.single { it.id == "ask" }
        assertTrue("the malformed short row stays as a stub", card.isStoredStub)
        assertTrue("the card says it could not be read", card.approvalReadFailed)
        assertTrue("its healthy neighbour still loads", kept.any { it.id == neighbor.id })
    }

    @Test fun anApprovalTooLargeForTheCursorDoesNotAbortTheSnapshot() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
        })
        // Corrupt it as well as making it huge, so the test fails whether the cursor read or the decode is the first error.
        val huge = WireJson.encodeToString(approval).dropLast(1)
        val neighbor = chat("bot", 2)
        mirror.receive(listOf(neighbor))
        mirror.close()
        val path = File(directory, "mirror-$computer.db").absolutePath
        SQLiteDatabase.openDatabase(path, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.insertOrThrow("entries", null, ContentValues().apply {
                put("id", approval.id); put("bot_id", approval.botId); put("seq", approval.seq); put("rev", approval.rev)
                put("payload", huge)
            })
            val thrown = runCatching {
                db.rawQuery("SELECT payload FROM entries WHERE id=?", arrayOf(approval.id)).use { it.moveToFirst(); it.getString(0) }
            }.isFailure
            assertTrue("the row is larger than the cursor window the production read uses", thrown)
        }
        mirror = MirrorDatabase(context, directory, computer)
        val kept = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = kept.single { it.id == "ask" }
        assertTrue("the oversized approval stays as a stub", card.isStoredStub)
        assertTrue("the card says it could not be read", card.approvalReadFailed)
        assertTrue("its healthy neighbour still loads", kept.any { it.id == neighbor.id })
    }

    @Test fun anUnreadableThreadApprovalStaysInItsThreadAsANonActionableWarning() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            // The host writes the body first and marks the thread after it: neither the stored head nor the tail names it.
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
            put("diffs", buildJsonArray {})
            put("author", "bot")
            putJsonObject("thread") { put("id", "thread-1"); put("title", "A thread's approval") }
            put("task", "task-1")
        })
        val huge = WireJson.encodeToString(approval).dropLast(1)
        assertTrue("the stored fragment ends after a complete thread object",
            huge.endsWith("\"task\":\"task-1\"},\"createdAt\":0,\"updatedAt\":0"))
        // The route is the column the schema wrote beside the row, so it survives a payload that no longer reads.
        storeRaw(approval, huge, thread = "thread-1")
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertTrue("the oversized thread approval stays as a stub", card.isStoredStub)
        assertTrue("the card says it could not be read", card.approvalReadFailed)
        assertTrue("no answers are offered from an unreadable card", (card.data["options"] as? JsonArray).isNullOrEmpty())
        assertEquals("the warning keeps its thread association",
            "thread-1", ((card.data["thread"] as JsonObject)["id"] as JsonPrimitive).content)
        val shown = threadChatItems(emptyList(), entries, "thread-1", emptyList(), live = false)
            .filterIsInstance<ChatItem.Message>()
        assertEquals(listOf("ask"), shown.map { it.entry.id })
        assertTrue(shown.single().entry.approvalReadFailed)
    }

    @Test fun aThreadMarkerInsideAnUnreadableRowsTextIsNotARoute() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
            putJsonObject("thread") { put("id", "thread-cut"); put("title", "A thread's approval") }
        })
        val full = WireJson.encodeToString(approval)
        val marker = "\"thread\":{\"id\":\"thread-cut\""
        val at = full.indexOf(marker)
        assertTrue("the fixture names the thread after the body", at > PAYLOAD_HEAD_CHARS)
        // The row is cut inside the thread object and no route was stored with it: its text is never read for one.
        val payload = full.substring(0, at + marker.length) + ",\"title\":\"A thr"
        storeRaw(approval, payload)
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertTrue("the cut approval stays as a stub", card.isStoredStub)
        assertTrue("the card says it could not be read", card.approvalReadFailed)
        assertNull("a thread object that never closes names no route", card.data["thread"])
        assertTrue("the warning stays in the owner chat",
            threadChatItems(emptyList(), entries, "thread-cut", emptyList(), live = false).none {
                it is ChatItem.Message && it.entry.id == "ask"
            })
    }

    @Test fun aReadableThreadApprovalKeepsItsGenuineThreadAndAnswers() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(64_000))
            putJsonObject("thread") { put("id", "thread-1") }
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
        })
        mirror.receive(listOf(approval))
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertFalse("its whole row reads, so it is not a stub", card.isStoredStub)
        assertEquals("the stored route has it", "thread-1",
            ((card.data["thread"] as JsonObject)["id"] as JsonPrimitive).content)
        assertEquals("its answers stay on the phone", "pending", card.status)
        assertEquals(listOf("ask"), threadChatItems(emptyList(), entries, "thread-1", emptyList(), live = false)
            .filterIsInstance<ChatItem.Message>().map { it.entry.id })
    }

    @Test fun aNulHiddenRoutingIdIsSkippedWithoutBreakingItsNeighbours() {
        val before = chat("bot", 1)
        val after = chat("bot", 3)
        val approval = Entry("ask", 2, "bot", rev = 2, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
        })
        mirror.receive(listOf(before, approval, after))
        mirror.close()
        // A route that got into the column somehow: SQLite's length() stops at the NUL, so this short-looking id
        // would project three million characters into the cursor window.
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.execSQL("UPDATE entries SET thread_id=? WHERE id=?", arrayOf("A" + '\u0000' + "X".repeat(3_000_000), approval.id))
        }
        mirror = MirrorDatabase(context, directory, computer)
        assertEquals("the schema this build writes", 6, mirror.readableDatabase.version)
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        assertEquals("both healthy neighbours stay visible", setOf(before.id, after.id), entries.map { it.id }.toSet() - "ask")
        val card = entries.single { it.id == "ask" }
        assertTrue("the untrustworthy metadata keeps the read-only stub", card.isStoredStub)
        assertNull("an id this mirror cannot trust is no route", card.data["thread"])
        assertNull("no answers are offered under unproven metadata", card.data["options"])
        assertTrue("the warning stays in the owner chat",
            threadChatItems(emptyList(), entries, "A", emptyList(), live = false).none {
                it is ChatItem.Message && it.entry.id == "ask"
            })
    }

    @Test fun aNestedThreadMarkerWithoutAGenuineOneStaysInTheOwnerChat() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
            putJsonObject("metadata") { putJsonObject("thread") { put("id", "thread-nested") } }
        })
        storeRaw(approval, WireJson.encodeToString(approval).dropLast(1))
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertTrue("the nested marker is not routing metadata", card.data["thread"] == null)
        assertTrue("the warning stays in the owner chat",
            threadChatItems(emptyList(), entries, "thread-nested", emptyList(), live = false).none {
                it is ChatItem.Message && it.entry.id == "ask"
            })
    }

    @Test fun aNestedThreadMarkerCannotReplaceTheStoredThread() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
            putJsonObject("thread") { put("id", "thread-real") }
            putJsonObject("metadata") { putJsonObject("thread") { put("id", "thread-nested") } }
            // Both thread keys follow the body, so they would otherwise sit in the last PAYLOAD_TAIL_CHARS,
            // the slice a stub is allowed to read. The trailer keeps them in the unread middle.
            put("trailer", "T".repeat(PAYLOAD_TAIL_CHARS))
        })
        val broken = WireJson.encodeToString(approval).dropLast(1)
        val nested = "\"thread\":{\"id\":\"thread-nested\""
        val nestedAt = broken.indexOf(nested)
        assertTrue("the nested marker sits in the unread middle",
            nestedAt > PAYLOAD_HEAD_CHARS && nestedAt + nested.length <= broken.length - PAYLOAD_TAIL_CHARS)
        assertFalse("the stored head does not name the thread", broken.take(PAYLOAD_HEAD_CHARS).contains("\"thread\""))
        assertFalse("the stored tail does not name the thread", broken.takeLast(PAYLOAD_TAIL_CHARS).contains("\"thread\""))
        mirror.receive(listOf(approval))
        mirror.close()
        // The payload is damaged afterwards, as a half-written row would be, and the stored route is all that is left.
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.execSQL("UPDATE entries SET payload=? WHERE id=?", arrayOf(broken, approval.id))
        }
        mirror = MirrorDatabase(context, directory, computer)
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertEquals("thread-real", ((card.data["thread"] as JsonObject)["id"] as JsonPrimitive).content)
        assertEquals(listOf("ask"), threadChatItems(emptyList(), entries, "thread-real", emptyList(), live = false)
            .filterIsInstance<ChatItem.Message>().map { it.entry.id })
        assertTrue("the nested thread does not receive the warning",
            threadChatItems(emptyList(), entries, "thread-nested", emptyList(), live = false).none {
                it is ChatItem.Message && it.entry.id == "ask"
            })
    }

    @Test fun aMalformedThreadIdDoesNotRollBackHealthyNeighbourWrites() {
        val before = chat("bot", 1)
        val after = chat("bot", 3)
        val malformed = Entry("ask", 2, "bot", rev = 2, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
            putJsonObject("thread") { put("id", buildJsonObject { put("nested", "thread-9") }) }
        })
        // The whole batch is one transaction: a routing value that is not a string must not throw and undo its neighbours.
        mirror.receive(listOf(before, malformed, after))
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        assertEquals("healthy rows on both sides are committed", setOf(before.id, malformed.id, after.id),
            entries.map { it.id }.toSet())
        val card = entries.single { it.id == malformed.id }
        assertTrue("the oversized approval remains as a warning", card.isStoredStub)
        assertTrue("the warning says its row could not be read", card.approvalReadFailed)
        assertNull("an object is not a routing id", card.data["thread"])
        assertTrue("an unreadable card offers no answer", (card.data["options"] as? JsonArray).isNullOrEmpty())
    }

    @Test fun anOversizedRoutingIdCannotBreakTheSnapshotOrHealthyNeighbours() {
        val before = chat("bot", 1)
        val after = chat("bot", 3)
        val oversized = Entry("ask", 2, "bot", rev = 2, kind = "permission", data = buildJsonObject {
            put("title", "Edit"); put("status", "pending")
            putJsonObject("thread") { put("id", "T".repeat(3_000_000)) }
            put("detail", "D".repeat(64_000))
        })
        mirror.receive(listOf(before, oversized, after))
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READONLY).use { db ->
            db.rawQuery("SELECT thread_id FROM entries WHERE id=?", arrayOf(oversized.id)).use {
                assertTrue(it.moveToFirst())
                assertTrue("a huge route id is not stored in the column", it.isNull(0))
            }
        }
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        assertEquals("both healthy neighbours still load", setOf(before.id, after.id), entries.map { it.id }.toSet() - oversized.id)
        val card = entries.single { it.id == oversized.id }
        assertTrue(card.isApprovalStub)
        assertNull("a huge id is not routing metadata", card.data["thread"])
        assertTrue("an unreadable card offers no answer", (card.data["options"] as? JsonArray).isNullOrEmpty())
    }

    @Test fun anOversizedApprovalsRouteComesFromTheColumnNotItsEdges() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            // Larger than the resident budget, smaller than the cursor window: the stub stays, and the read does not fail.
            put("detail", "D".repeat(200_000))
            putJsonObject("thread") { put("id", "thread-middle") }
            put("after", "N".repeat(512))
        })
        val payload = WireJson.encodeToString(approval)
        assertFalse("the stored head does not name the thread", payload.take(PAYLOAD_HEAD_CHARS).contains("\"thread\""))
        assertFalse("the stored tail does not name the thread", payload.takeLast(PAYLOAD_TAIL_CHARS).contains("\"thread\""))
        mirror.receive(listOf(approval))
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertTrue("the oversized approval's body is not read whole", card.isStoredStub)
        assertFalse("its whole row reads, so the card is not a warning", card.approvalReadFailed)
        assertEquals("the route stored beside the row is the one it keeps", "thread-middle",
            ((card.data["thread"] as JsonObject)["id"] as JsonPrimitive).content)
        assertEquals(listOf("ask"), threadChatItems(emptyList(), entries, "thread-middle", emptyList(), live = false)
            .filterIsInstance<ChatItem.Message>().map { it.entry.id })
    }

    @Test fun anAnsweredOversizedApprovalIsNotReplacedAndStaysInsideTheWindowBudget() {
        val filler = "A".repeat(200_000)
        val answered = Entry("answered", 2, "bot", rev = 2, kind = "permission", data = buildJsonObject {
            put("status", "answered")
            put("selected", "allow")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
            put("detail", filler)
        })
        val pending = Entry("pending", 3, "bot", rev = 3, kind = "permission", data = buildJsonObject {
            put("status", "pending")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
            put("detail", "P".repeat(200_000))
        })
        val fits = Entry("fits", 4, "bot", rev = 4, kind = "permission", data = buildJsonObject {
            put("status", "pending")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
            put("detail", "Q".repeat(PER_MESSAGE_CHARS + 1))
        })
        mirror.receive(listOf(chat("bot", 1), answered, pending, fits))
        val kept = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries.filter { it.botId == "bot" }
        val card = kept.single { it.id == "answered" }
        assertTrue("an answered card is not replaced by its full payload", card.isStoredStub)
        assertFalse(filler in card.data.toString())
        assertEquals("answered", card.status)
        val waiting = kept.single { it.id == "pending" }
        assertTrue("a pending card past the window budget stays a stub", waiting.isStoredStub)
        assertEquals("pending", waiting.status)
        val answerable = kept.single { it.id == "fits" }
        assertFalse("a pending card that fits is still read whole", answerable.isStoredStub)
        assertEquals("pending", answerable.status)
        // window 10 uses the resident floor: max(160_000, 10 * 2_000).
        assertTrue(kept.sumOf { WireJson.encodeToString(it).length.toLong() } <= 160_000L)
    }

    @Test fun anOversizedApprovalComesBackWholeSoItsAnswersStayOnThePhone() {
        val filler = "D".repeat(PER_MESSAGE_CHARS + 1)
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("toolKind", "edit")
            put("detail", filler)
            put("status", "pending")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
                add(buildJsonObject { put("optionId", "deny"); put("name", "Deny"); put("kind", "reject_once") })
            })
        })
        mirror.receive(listOf(approval))
        val kept = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries.single()
        assertFalse("the card answers from its own row, not its head", kept.isStoredStub)
        assertEquals("pending", kept.status)
        assertEquals(listOf("allow", "deny"), (kept.data["options"] as JsonArray).map { (it as JsonObject).text("optionId") })
    }

    private fun chat(bot: String, seq: Long): Entry = Entry("e-$bot-$seq", seq, bot, rev = seq, kind = "user",
        data = buildJsonObject { put("text", "$bot-$seq"); put("clientNonce", "n-$bot-$seq") })

    /** A chat row whose route is [thread], or none; [filler] makes it a row too large to read whole. */
    private fun routed(seq: Long, thread: String?, bot: String = "bot", filler: Int = 0): Entry =
        Entry("e-$bot-$seq", seq, bot, rev = seq, kind = "user", data = buildJsonObject {
            put("text", if (filler > 0) "M".repeat(filler) else "$bot-$seq")
            put("clientNonce", "n-$bot-$seq")
            if (thread != null) putJsonObject("thread") { put("id", thread) }
        })

    private fun queued(nonce: String = "unsent", text: String = "aside", createdAt: Long = 1) =
        PendingSend(nonce, "bot", text = text, createdAt = createdAt, status = SendStatus.Held)

    /** The ids [entries] shows in thread [thread]. */
    private fun inThread(entries: List<Entry>, thread: String): List<String> =
        threadChatItems(emptyList(), entries, thread, emptyList(), live = false)
            .filterIsInstance<ChatItem.Message>().map { it.entry.id }

    @Test fun anUpgradeFromEveryOlderSchemaDropsTheCacheAndKeepsWhatWasNotFetched() {
        for (version in listOf(2, 3, 4, 5)) {
            storeOldMirror(
                version = version,
                rows = listOf(chat("bot", 1) to WireJson.encodeToString(chat("bot", 1))),
                bots = listOf(Bot("bot", "Bot", rev = 2)),
                pending = listOf(queued()),
            )
            assertEquals("$version upgrades to the schema this build writes", 6, mirror.readableDatabase.version)
            val snapshot = mirror.snapshot()
            assertTrue("$version: the cached conversation goes", snapshot.entries.isEmpty())
            assertEquals("$version: the cursor starts over", 0, snapshot.rev)
            assertFalse("$version: the chat pages its history again", mirror.historyComplete("bot"))
            assertEquals("$version: the roster stays", listOf("bot"), snapshot.bots.map { it.id })
            assertEquals("$version: the unsent message stays", "aside", snapshot.pending.single().text)
        }
    }

    @Test fun anUpgradeWithMissingTablesOrACorruptRowStillOpens() {
        // A cache whose conversation tables never existed: nothing to drop, and no row to read.
        storeOldMirror(version = 2, withConversation = false, bots = listOf(Bot("bot", "Bot", rev = 2)), pending = listOf(queued()))
        assertEquals(6, mirror.readableDatabase.version)
        assertTrue(mirror.snapshot().entries.isEmpty())
        assertEquals("the unsent message survives a cache with no conversation tables",
            listOf("unsent"), mirror.snapshot().pending.map { it.nonce })
        assertFalse("a missing conversation is recreated empty, not left half-dropped", mirror.historyComplete("bot"))
        // A payload that no longer reads, and no meta table to write the cursor into.
        storeOldMirror(version = 5, rows = listOf(chat("bot", 1) to "not a payload"), withMeta = false)
        assertEquals(6, mirror.readableDatabase.version)
        assertEquals("the upgrade writes the cursor it could not find", 0, mirror.revision())
        assertTrue(mirror.snapshot().entries.isEmpty())
        assertFalse("the corrupt row and its history lane are both gone", mirror.historyComplete("bot"))
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READONLY).use { db ->
            val columns = db.rawQuery("PRAGMA table_info(entries)", null).use { info ->
                buildList { while (info.moveToNext()) add(info.getString(1)) }
            }
            assertTrue("the new conversation table is whole", "thread_id" in columns && "payload" in columns)
            assertFalse("the compatibility column is not kept", "routing_known" in columns)
            assertEquals(0, db.compileStatement("SELECT COUNT(*) FROM entries").simpleQueryForLong())
            assertEquals(0, db.compileStatement("SELECT COUNT(*) FROM history").simpleQueryForLong())
        }
        // Opening it again does not run the upgrade a second time.
        mirror.close()
        mirror = MirrorDatabase(context, directory, computer)
        assertEquals(6, mirror.readableDatabase.version)
        assertEquals(0, mirror.revision())
    }

    @Test fun aReconnectAfterTheUpgradeBringsTheConversationBackWithItsRoutes() {
        val stale = routed(1, "thread-old")
        storeOldMirror(version = 4, rows = listOf(stale to WireJson.encodeToString(stale)))
        assertTrue("the old cache is dropped before the reconnect", mirror.snapshot().entries.isEmpty())

        val short = routed(2, "thread-1")
        val large = routed(3, "thread-2", filler = PER_MESSAGE_CHARS + 1)
        // The reconnect: a hello, then the catch-up and history pages the computer sends from rev 0.
        mirror.apply(MirrorEvent.Hello("host", 0, JsonObject(emptyMap()), null))
        mirror.receive(listOf(short, large))
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        assertEquals("the conversation comes back", listOf(short.id, large.id), entries.map { it.id })
        assertEquals("the small row keeps its route", listOf(short.id), inThread(entries, "thread-1"))
        assertEquals("the large row keeps its route", listOf(large.id), inThread(entries, "thread-2"))
        mirror.receive(listOf(short, large))
        assertEquals("the same page a second time does not duplicate them", listOf(short.id, large.id),
            mirror.snapshot(Residency(setOf("bot"), window = 10)).entries.map { it.id })
        assertFalse("the large row's edges cannot name its thread", WireJson.encodeToString(large).take(PAYLOAD_HEAD_CHARS)
            .contains("\"thread\""))
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READONLY).use { db ->
            db.rawQuery("SELECT thread_id FROM entries WHERE id=?", arrayOf(large.id)).use {
                assertTrue(it.moveToFirst())
                assertEquals("its route is the column, not its text", "thread-2", it.getString(0))
            }
        }
    }

    @Test fun aDuplicateOrStaleRefetchDoesNotDuplicateOrOvershadowARow() {
        val older = Entry("stale", 1, "bot", rev = 1, kind = "user", data = buildJsonObject {
            put("text", "stale"); put("clientNonce", "same")
        })
        val copy = Entry("copy", 2, "bot", rev = 2, kind = "user", data = buildJsonObject {
            put("text", "also stale"); put("clientNonce", "same")
        })
        storeOldMirror(version = 3, rows = listOf(
            older to WireJson.encodeToString(older),
            copy to WireJson.encodeToString(copy),
        ))
        assertTrue("duplicate and stale rows in the old cache are dropped together", mirror.snapshot().entries.isEmpty())

        mirror.receive(listOf(routed(1, null), routed(2, null)))
        // The same page twice.
        mirror.receive(listOf(routed(1, null), routed(2, null)))
        assertEquals(listOf(1L, 2L), mirror.snapshot().entries.map { it.seq })

        mirror.receive(listOf(routed(2, null).copy(rev = 9, data = buildJsonObject { put("text", "newer") })))
        assertEquals("a newer copy replaces the one held", listOf("newer"),
            mirror.snapshot().entries.filter { it.seq == 2L }.map { it.text })
        // A page from before it, and a live event replayed behind it: neither may resurrect the old text.
        mirror.receive(listOf(routed(2, null).copy(rev = 4, data = buildJsonObject { put("text", "stale") })))
        mirror.apply(MirrorEvent.EntryChanged(routed(2, null).copy(rev = 5, data = buildJsonObject { put("text", "replay") })))
        assertEquals("a stale copy neither duplicates nor overshadows the row", listOf("newer"),
            mirror.snapshot().entries.filter { it.seq == 2L }.map { it.text })
        assertEquals("no row is duplicated", 2, mirror.snapshot().entries.size)
        // A whole chat fetched again out of order ends with each row once.
        mirror.receive(listOf(routed(3, null), routed(1, null), routed(2, null)))
        assertEquals(listOf(1L, 2L, 3L), mirror.snapshot().entries.map { it.seq })
    }

    @Test fun anOfflineStartAfterTheUpgradeShowsNoHistoryAndStillSendsItsQueuedMessage() {
        storeOldMirror(
            version = 5,
            rows = listOf(chat("bot", 1) to WireJson.encodeToString(chat("bot", 1))),
            bots = listOf(Bot("bot", "Bot", rev = 2)),
            pending = listOf(queued()),
        )
        val snapshot = mirror.snapshot()
        assertTrue("offline, the chat is empty and the app is not", snapshot.entries.isEmpty())
        assertFalse("the chat asks the computer for its history again", mirror.historyComplete("bot"))
        assertFalse("there is no row left to page back to", mirror.hasOlder("bot", Long.MAX_VALUE))
        assertEquals("the queued message is still listed", listOf("unsent"), snapshot.pending.map { it.nonce })
        assertEquals(SendStatus.Held, snapshot.pending.single().status)
        // It is still sendable: a second one joins it, in the order they were written.
        mirror.save(queued(nonce = "second", text = "second", createdAt = 2))
        assertEquals(listOf("aside", "second"), mirror.snapshot().pending.map { it.text })
        // The computer confirms the first: the queued copy goes when its message arrives.
        mirror.apply(MirrorEvent.EntryChanged(chat("bot", 2).copy(data = buildJsonObject {
            put("text", "aside"); put("clientNonce", "unsent")
        })))
        assertEquals(listOf("second"), mirror.snapshot().pending.map { it.text })
        assertTrue("its message is the one now in the chat", mirror.hasNonce("bot", "unsent"))
    }

    @Test fun aFailingApprovalReadIsReportedOnce() {
        armAnalytics()
        val approval = Entry("once-approval", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
        })
        val huge = WireJson.encodeToString(approval).dropLast(1)
        mirror.receive(listOf(chat("bot", 2)))
        mirror.close()
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.insertOrThrow("entries", null, ContentValues().apply {
                put("id", approval.id); put("bot_id", approval.botId); put("seq", approval.seq); put("rev", approval.rev)
                put("payload", huge)
            })
        }
        mirror = MirrorDatabase(context, directory, computer)
        val window = Residency(setOf("bot"), window = 10)
        mirror.snapshot(window)
        mirror.snapshot(window)
        assertEquals("two snapshots of a row that cannot be read send one event", 1, reported("approval_read_failed"))
        assertTrue(mirror.snapshot(window).entries.single { it.id == approval.id }.approvalReadFailed)
    }

    @Test fun aFailingStoredMessageReadIsReportedOnce() {
        armAnalytics()
        val bad = "{not-json"
        repeat(2) { assertTrue(runCatching { storedTextOfPayload(bad, "once-stored") }.isFailure) }
        assertEquals("two reads of a payload that will not decode send one event", 1, reported("stored_message_read_failed"))
        val blank = WireJson.encodeToString(chat("bot", 3).copy(data = buildJsonObject { put("text", "   ") }))
        repeat(2) { assertTrue(runCatching { storedTextOfPayload(blank, "once-blank") }.isFailure) }
        assertEquals("a row with no text is its own failure, once", 2, reported("stored_message_read_failed"))
    }

    @Test fun aRowBiggerThanTheSheetReadsOpensWithItsBound() {
        armAnalytics()
        val approval = Entry("once-open", 1, "bot", rev = 1, kind = "user", data = buildJsonObject {
            put("text", "D".repeat(3_000_000))
        })
        mirror.receive(listOf(chat("bot", 2)))
        mirror.close()
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.insertOrThrow("entries", null, ContentValues().apply {
                put("id", approval.id); put("bot_id", approval.botId); put("seq", approval.seq); put("rev", approval.rev)
                put("payload", WireJson.encodeToString(approval))
            })
        }
        mirror = MirrorDatabase(context, directory, computer)
        val store = previewStore()
        store.database = mirror
        // Bigger than one cursor window: the sheet reads its bound, not the row, so the message opens.
        val shown = store.storedMessageText(approval)
        assertTrue(shown.startsWith("DDDD"))
        assertTrue("the sheet reads at most one message's characters", shown.length <= PER_MESSAGE_CHARS)
        assertEquals("the sheet says the rest of the row was not read", STORED_CUT_MARK, shown.takeLast(1))
        assertEquals("a message that opened reports nothing", 0, reported("stored_message_read_failed"))
    }

    private fun armAnalytics() {
        ReadFailureReports.reset()
        Analytics.start(context)
        Analytics.setEnabled(true)
        Analytics.pending.clear()
    }

    private fun reported(name: String) = Analytics.pending.count {
        it["event"]?.jsonPrimitive?.contentOrNull == name
    }
}
