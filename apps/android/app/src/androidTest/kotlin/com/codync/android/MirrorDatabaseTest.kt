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

    /** Stores [payload] exactly as an older app could have left it, before routing metadata was kept beside the row. */
    /** Replaces the mirror with a version 3 cache holding exact payloads, as an older app left them. */
    private fun storeVersion3(rows: List<Pair<Entry, String>>) {
        mirror.close()
        val path = File(directory, "mirror-$computer.db")
        path.delete()
        SQLiteDatabase.openOrCreateDatabase(path, null).use { db ->
            db.execSQL("CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT NOT NULL)")
            db.execSQL("CREATE TABLE bots(id TEXT PRIMARY KEY, rev INTEGER NOT NULL, deleted INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE entries(id TEXT PRIMARY KEY, bot_id TEXT NOT NULL, seq INTEGER NOT NULL, rev INTEGER NOT NULL, nonce TEXT, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE pending(nonce TEXT PRIMARY KEY, created INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE history(lane TEXT PRIMARY KEY)")
            db.insertOrThrow("meta", null, ContentValues().apply { put("k", "rev"); put("v", "1") })
            rows.forEach { (row, payload) ->
                db.insertOrThrow("entries", null, ContentValues().apply {
                    put("id", row.id); put("bot_id", row.botId); put("seq", row.seq); put("rev", row.rev)
                    put("payload", payload)
                })
            }
            db.version = 3
        }
        mirror = MirrorDatabase(context, directory, computer)
    }

    private fun storeRaw(approval: Entry, payload: String, neighbor: Entry = chat("bot", 2)) {
        mirror.receive(listOf(neighbor))
        mirror.close()
        SQLiteDatabase.openDatabase(File(directory, "mirror-$computer.db").absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.insertOrThrow("entries", null, ContentValues().apply {
                put("id", approval.id); put("bot_id", approval.botId); put("seq", approval.seq); put("rev", approval.rev)
                put("payload", payload)
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
        assertTrue(small.all { filler !in it.text && it.text.length < 200 && notedLength(it.text) > 60_000 })
        assertTrue(wide.all { filler !in it.text && it.text.length < 200 && notedLength(it.text) > 60_000 })
        val preview = mirror.snapshot(Residency(emptySet(), window = 300, preview = 30)).entries.filter { it.botId == "big" }
        assertEquals("a preview keeps the newest stubs and not the 60k filler", 30, preview.size)
        assertEquals(600L, preview.maxOf { it.seq })
        assertTrue(preview.all { filler !in it.text && it.text.length < 200 && notedLength(it.text) > 60_000 })
    }

    private fun notedLength(text: String) = text.substringAfter("(", "").substringBefore(" ", "").toLongOrNull() ?: 0L

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
            // The host writes the body first and marks the thread after it: a truncated stored head alone misses the marker.
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
            put("diffs", buildJsonArray {})
            put("author", "bot")
            putJsonObject("thread") { put("id", "thread-1"); put("title", "A thread's approval") }
            put("task", "task-1")
        })
        val huge = WireJson.encodeToString(approval).dropLast(1)
        // The stored row stops after a complete thread object: only the root object's last brace is missing.
        assertTrue("the stored fragment ends after a complete thread object",
            huge.contains("\"thread\":{\"id\":\"thread-1\",\"title\":\"A thread's approval\"}") &&
                huge.endsWith("\"task\":\"task-1\"},\"createdAt\":0,\"updatedAt\":0"))
        val neighbor = chat("bot", 2)
        mirror.receive(listOf(neighbor))
        mirror.close()
        val path = File(directory, "mirror-$computer.db").absolutePath
        SQLiteDatabase.openDatabase(path, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.insertOrThrow("entries", null, ContentValues().apply {
                put("id", approval.id); put("bot_id", approval.botId); put("seq", approval.seq); put("rev", approval.rev)
                put("payload", huge)
            })
        }
        mirror = MirrorDatabase(context, directory, computer)
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

    @Test fun aLegacyThreadCutBeforeItsObjectClosesStaysInTheOwnerChat() {
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
        // The row is cut inside the thread object, so nothing on it proves where it belongs.
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

    @Test fun aReadableDuplicateThreadRouteKeepsItsApprovalOutOfBothThreads() {
        // 64,000 characters: past the window's head sample, but the whole row still decodes.
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(64_000))
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
        })
        val payload = WireJson.encodeToString(approval)
            .replaceFirst("\"data\":{", "\"data\":{\"thread\":{\"id\":\"thread-a\",\"id\":\"thread-b\"},")
        assertTrue("the row is small enough to read whole", payload.length < 2 * 64 * 1_024)
        storeVersion3(listOf(approval to payload))
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertTrue("the route is not proven, so the card keeps its read-only stub", card.isStoredStub)
        assertNull("the decoder's last duplicate id is not a route", card.data["thread"])
        assertNull("no answers are offered under an unproven thread", card.data["options"])
        for (thread in listOf("thread-a", "thread-b")) {
            assertTrue("the card is not offered under $thread",
                threadChatItems(emptyList(), entries, thread, emptyList(), live = false).none {
                    it is ChatItem.Message && it.entry.id == "ask"
                })
        }
    }

    @Test fun aSmallReadableDuplicateThreadRouteAlsoStaysInTheOwnerChat() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(1_000))
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
        })
        val payload = WireJson.encodeToString(approval)
            .replaceFirst("\"data\":{", "\"data\":{\"thread\":{\"id\":\"thread-a\",\"id\":\"thread-b\"},")
        storeVersion3(listOf(approval to payload))
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertNull("the duplicate direct id proves no route", card.data["thread"])
        assertNull("no answers are offered under an unproven thread", card.data["options"])
        for (thread in listOf("thread-a", "thread-b")) {
            assertTrue("the card is not offered under $thread",
                threadChatItems(emptyList(), entries, thread, emptyList(), live = false).none {
                    it is ChatItem.Message && it.entry.id == "ask"
                })
        }
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
        storeVersion3(listOf(approval to WireJson.encodeToString(approval)))
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertFalse("its whole row reads, so it is not a stub", card.isStoredStub)
        assertEquals("the genuine direct marker still routes it", "thread-1",
            ((card.data["thread"] as JsonObject)["id"] as JsonPrimitive).content)
        assertEquals("its answers stay on the phone", "pending", card.status)
        assertEquals(listOf("ask"), threadChatItems(emptyList(), entries, "thread-1", emptyList(), live = false)
            .filterIsInstance<ChatItem.Message>().map { it.entry.id })
    }

    @Test fun aNulHiddenRoutingIdIsSkippedWithoutBreakingItsNeighbours() {
        mirror.close()
        val path = File(directory, "mirror-$computer.db")
        path.delete()
        val before = chat("bot", 1)
        val after = chat("bot", 3)
        val approval = Entry("ask", 2, "bot", rev = 2, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
        })
        fun row(entry: Entry, thread: String?) = ContentValues().apply {
            put("id", entry.id); put("bot_id", entry.botId); put("thread_id", thread)
            put("seq", entry.seq); put("rev", entry.rev); put("payload", WireJson.encodeToString(entry))
        }
        SQLiteDatabase.openOrCreateDatabase(path, null).use { db ->
            db.execSQL("CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT NOT NULL)")
            db.execSQL("CREATE TABLE bots(id TEXT PRIMARY KEY, rev INTEGER NOT NULL, deleted INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE entries(id TEXT PRIMARY KEY, bot_id TEXT NOT NULL, thread_id TEXT, seq INTEGER NOT NULL, rev INTEGER NOT NULL, nonce TEXT, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE pending(nonce TEXT PRIMARY KEY, created INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE history(lane TEXT PRIMARY KEY)")
            db.insertOrThrow("meta", null, ContentValues().apply { put("k", "rev"); put("v", "3") })
            // SQLite's length() stops at the NUL, so this short-looking id used to project three million characters.
            db.insertOrThrow("entries", null, row(before, null))
            db.insertOrThrow("entries", null, row(approval, "A" + '\u0000' + "X".repeat(3_000_000)))
            db.insertOrThrow("entries", null, row(after, null))
            db.version = 4
        }
        mirror = MirrorDatabase(context, directory, computer)
        assertEquals("the additive upgrade lands on schema 5", 5, mirror.readableDatabase.version)
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

    @Test fun aNestedThreadMarkerCannotReplaceTheGenuineThreadThatPrecedesIt() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
            putJsonObject("thread") { put("id", "thread-real") }
            putJsonObject("metadata") { putJsonObject("thread") { put("id", "thread-nested") } }
        })
        storeRaw(approval, WireJson.encodeToString(approval).dropLast(1))
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

    @Test fun aLegacyDuplicateIdInTheHeadStaysUnroutedThroughTheRealSnapshot() {
        val before = chat("bot", 2)
        val after = chat("bot", 3)
        val payload = """{"id":"ask","seq":1,"botId":"bot","rev":1,"kind":"permission","data":{"title":"Edit","status":"pending","thread":{"id":"thread-a","id":"thread-b"},"detail":"""" +
            "D".repeat(3_000_000) + """"}}"""
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject { put("title", "Edit") })
        assertTrue(payload.take(PAYLOAD_HEAD_CHARS).contains("\"id\":\"thread-b\""))
        storeVersion3(listOf(before to WireJson.encodeToString(before), approval to payload, after to WireJson.encodeToString(after)))
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        assertEquals("healthy rows survive on both sides", setOf(before.id, approval.id, after.id), entries.map { it.id }.toSet())
        val card = entries.single { it.id == "ask" }
        assertTrue(card.isApprovalStub)
        assertTrue(card.approvalReadFailed)
        assertNull("two direct ids are ambiguous even when one survives a lenient decode", card.data["thread"])
        for (thread in listOf("thread-a", "thread-b")) {
            assertTrue("the warning is not routed to $thread",
                threadChatItems(emptyList(), entries, thread, emptyList(), live = false).none {
                    it is ChatItem.Message && it.entry.id == "ask"
                })
        }
    }

    @Test fun anEscapedLegacyThreadKeyRoutesAndIsScannedOnlyOnce() {
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
            putJsonObject("thread") { put("id", "thread-escaped") }
            put("after", "N".repeat(512))
        })
        val payload = WireJson.encodeToString(approval).replaceFirst("\"thread\":", "\"th\\u0072ead\":")
        assertFalse("the literal key is absent from both sampled edges", payload.take(PAYLOAD_HEAD_CHARS).contains("\"thread\""))
        assertFalse(payload.takeLast(PAYLOAD_TAIL_CHARS).contains("\"thread\"") || payload.takeLast(PAYLOAD_TAIL_CHARS).contains("th\\u0072ead"))
        storeVersion3(listOf(approval to payload))
        val baseline = mirror.routingScanAttempts
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertEquals("the escaped key is a genuine direct marker", "thread-escaped",
            ((card.data["thread"] as JsonObject)["id"] as JsonPrimitive).content)
        assertEquals("one structural scan", baseline + 1, mirror.routingScanAttempts)
        mirror.snapshot(Residency(setOf("bot"), window = 10))
        assertEquals("a cached positive route is not scanned again", baseline + 1, mirror.routingScanAttempts)
    }

    @Test fun aLegacyNegativeRouteIsScannedOnceAndANewNullRouteNeedsNoScan() {
        val legacy = Entry("legacy-ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit"); put("status", "pending"); put("detail", "D".repeat(40_000))
        })
        val brokenLegacy = WireJson.encodeToString(legacy).dropLast(1)
        storeVersion3(listOf(legacy to brokenLegacy))
        val baseline = mirror.routingScanAttempts
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        assertNull("an unthreaded legacy warning stays in the owner chat", entries.single().data["thread"])
        assertEquals("the negative route is scanned once", baseline + 1, mirror.routingScanAttempts)
        mirror.snapshot(Residency(setOf("bot"), window = 10))
        assertEquals("the cached negative route is not scanned again", baseline + 1, mirror.routingScanAttempts)
        val fresh = Entry("fresh-ask", 2, "bot", rev = 2, kind = "permission", data = buildJsonObject {
            put("title", "Edit"); put("status", "pending"); put("detail", "D".repeat(40_000))
        })
        mirror.receive(listOf(fresh))
        mirror.snapshot(Residency(setOf("bot"), window = 10))
        mirror.snapshot(Residency(setOf("bot"), window = 10))
        assertEquals("a route written as null by the current schema is already known", baseline + 1, mirror.routingScanAttempts)
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
            db.rawQuery("SELECT thread_id, routing_known FROM entries WHERE id=?", arrayOf(oversized.id)).use {
                assertTrue(it.moveToFirst())
                assertTrue("a huge route id is not stored in the column", it.isNull(0))
                assertEquals("the absence is authoritative", 1, it.getInt(1))
            }
        }
        val baseline = mirror.routingScanAttempts
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        assertEquals("both healthy neighbours still load", setOf(before.id, after.id), entries.map { it.id }.toSet() - oversized.id)
        val card = entries.single { it.id == oversized.id }
        assertTrue(card.isApprovalStub)
        assertNull("a huge id is not routing metadata", card.data["thread"])
        assertTrue("an unreadable card offers no answer", (card.data["options"] as? JsonArray).isNullOrEmpty())
        assertEquals("a written null route does not trigger a legacy scan", baseline, mirror.routingScanAttempts)
    }

    @Test fun aLegacyMarkerInTheMiddleOfAnOversizedApprovalReachesItsThread() {
        mirror.close()
        val path = File(directory, "mirror-$computer.db")
        path.delete()
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
            put("detail", "D".repeat(3_000_000))
            putJsonObject("thread") { put("id", "thread-middle") }
            put("after", "N".repeat(512))
        })
        val payload = WireJson.encodeToString(approval)
        assertFalse("the head does not name the thread", payload.take(PAYLOAD_HEAD_CHARS).contains("\"thread\""))
        assertFalse("the tail does not name the thread", payload.takeLast(PAYLOAD_TAIL_CHARS).contains("\"thread\""))
        SQLiteDatabase.openOrCreateDatabase(path, null).use { db ->
            db.execSQL("CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT NOT NULL)")
            db.execSQL("CREATE TABLE bots(id TEXT PRIMARY KEY, rev INTEGER NOT NULL, deleted INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE entries(id TEXT PRIMARY KEY, bot_id TEXT NOT NULL, seq INTEGER NOT NULL, rev INTEGER NOT NULL, nonce TEXT, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE pending(nonce TEXT PRIMARY KEY, created INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE history(lane TEXT PRIMARY KEY)")
            db.insertOrThrow("meta", null, ContentValues().apply { put("k", "rev"); put("v", "1") })
            db.insertOrThrow("entries", null, ContentValues().apply {
                put("id", approval.id); put("bot_id", approval.botId); put("seq", approval.seq); put("rev", approval.rev)
                put("payload", payload)
            })
            db.version = 3
        }
        mirror = MirrorDatabase(context, directory, computer)
        val entries = mirror.snapshot(Residency(setOf("bot"), window = 10)).entries
        val card = entries.single { it.id == "ask" }
        assertTrue("the v3 approval remains as a warning", card.isStoredStub)
        assertTrue("the warning says its row could not be read", card.approvalReadFailed)
        assertEquals("the middle marker routes the warning", "thread-middle",
            ((card.data["thread"] as JsonObject)["id"] as JsonPrimitive).content)
        assertEquals(listOf("ask"), threadChatItems(emptyList(), entries, "thread-middle", emptyList(), live = false)
            .filterIsInstance<ChatItem.Message>().map { it.entry.id })
    }

    @Test fun legacyRoutingRejectsDuplicateAndTruncatedDirectThreadIds() {
        val path = File(directory, "routing.db")
        val db = SQLiteDatabase.openOrCreateDatabase(path, null)
        db.execSQL("CREATE TABLE entries(id TEXT PRIMARY KEY, payload TEXT NOT NULL)")
        var next = 0
        fun route(json: String): String? {
            val id = "route-${next++}"
            db.insertOrThrow("entries", null, ContentValues().apply { put("id", id); put("payload", json) })
            return storedRoutingThread(db, id)
        }
        fun payload(thread: String) =
            """{"id":"ask","seq":4,"botId":"bot","rev":4,"kind":"permission",""" +
                """"data":{"title":"Edit","thread":$thread}}"""
        assertEquals("t", route(payload("""{"id":"t","title":"A thread"}""")))
        assertNull("two direct ids are ambiguous", route(payload("""{"id":"a","id":"b","title":"A thread"}""")))
        assertNull("an id before the object closes is ambiguous", route(
            "{\"id\":\"ask\",\"seq\":4,\"botId\":\"bot\",\"rev\":4,\"kind\":\"permission\"," +
                "\"data\":{\"thread\":{\"id\":\"a\",\"title\":\"A thread\""))
        assertEquals("a fragment that ends after a complete thread object keeps its route", "t", route(
            "{\"id\":\"ask\",\"seq\":4,\"botId\":\"bot\",\"rev\":4,\"kind\":\"permission\"," +
                "\"data\":{\"thread\":{\"id\":\"t\",\"title\":\"A thread\"}"))
        assertNull("a bracket closed by the wrong character is malformed", route(
            """{"id":"ask","seq":4,"botId":"bot","rev":4,"kind":"permission","data":{"metadata":[},"thread":{"id":"A"}}}"""))
        assertNull("a trailing comma is not a field", route(
            """{"id":"ask","seq":4,"botId":"bot","rev":4,"kind":"permission","data":{"thread":{"id":"A",}}}"""))
        assertNull("a non-string id is not routing metadata", route(payload("""{"id":{"nested":"t"},"title":"A thread"}""")))
        assertEquals("the direct thread wins over a nested one", "real", route(
            """{"id":"ask","seq":4,"botId":"bot","rev":4,"kind":"permission","data":{"metadata":{"thread":{"id":"nested"}},""" +
                """"thread":{"id":"real","title":"A thread"}}}"""))
        assertEquals("an escaped field name is still the direct key", "escaped", route(
            "{\"id\":\"ask\",\"seq\":4,\"botId\":\"bot\",\"rev\":4,\"kind\":\"permission\"," +
                "\"data\":{\"th\\u0072ead\":{\"id\":\"escaped\",\"title\":\"A thread\"}}}"))
        assertNull("two direct thread fields are ambiguous", route(
            """{"id":"ask","seq":4,"botId":"bot","rev":4,"kind":"permission",""" +
                """"data":{"thread":{"id":"a"},"thread":{"id":"b"}}}"""))
        assertNull("two root data fields are ambiguous", route(
            """{"id":"ask","seq":4,"botId":"bot","rev":4,"kind":"permission",""" +
                """"data":{"thread":{"id":"a"}},"data":{"thread":{"id":"b"}}}"""))
        assertNull("a multi-megabyte route id is malformed", route(
            """{"id":"ask","seq":4,"botId":"bot","rev":4,"kind":"permission","data":{"thread":{"id":"""" +
                "I".repeat(3_000_000) + """"}}}"""))
        assertNull("a huge thread key is malformed without being buffered", route(
            """{"id":"ask","seq":4,"botId":"bot","rev":4,"kind":"permission","data":{"thread":{"""" +
                "K".repeat(10_000) + """": "ignored","id":"t"}}}"""))
        db.close()
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

    @Test fun upgradingFromVersionThreeAddsRoutingAndKeepsTheConversation() {
        mirror.close()
        val path = File(directory, "mirror-$computer.db")
        path.delete()
        SQLiteDatabase.openOrCreateDatabase(path, null).use { db ->
            db.execSQL("CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT NOT NULL)")
            db.execSQL("CREATE TABLE bots(id TEXT PRIMARY KEY, rev INTEGER NOT NULL, deleted INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE entries(id TEXT PRIMARY KEY, bot_id TEXT NOT NULL, seq INTEGER NOT NULL, rev INTEGER NOT NULL, nonce TEXT, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE pending(nonce TEXT PRIMARY KEY, created INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE history(lane TEXT PRIMARY KEY)")
            db.insertOrThrow("meta", null, ContentValues().apply { put("k", "rev"); put("v", "3") })
            db.insertOrThrow("entries", null, ContentValues().apply {
                put("id", "message"); put("bot_id", "bot"); put("seq", 1); put("rev", 3)
                put("payload", WireJson.encodeToString(entry(3)))
            })
            db.insertOrThrow("history", null, ContentValues().apply { put("lane", "bot") })
            db.version = 3
        }
        mirror = MirrorDatabase(context, directory, computer)
        val snapshot = mirror.snapshot()
        assertEquals("the cached row survives the additive schema change", listOf("message"), snapshot.entries.map { it.id })
        assertEquals("the event cursor is kept", 3, snapshot.rev)
        assertTrue("the history completion is kept", mirror.historyComplete("bot"))
    }

    @Test fun upgradingTheMirrorFetchesTheConversationAgainAndKeepsUnsentMessages() {
        mirror.close()
        val path = File(directory, "mirror-$computer.db")
        path.delete()
        SQLiteDatabase.openOrCreateDatabase(path, null).use { db ->
            db.execSQL("CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT NOT NULL)")
            db.execSQL("CREATE TABLE bots(id TEXT PRIMARY KEY, rev INTEGER NOT NULL, deleted INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE entries(id TEXT PRIMARY KEY, bot_id TEXT NOT NULL, thread_id TEXT, seq INTEGER NOT NULL, rev INTEGER NOT NULL, nonce TEXT, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE pending(nonce TEXT PRIMARY KEY, created INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE history(lane TEXT PRIMARY KEY)")
            db.insertOrThrow("meta", null, ContentValues().apply { put("k", "rev"); put("v", "3") })
            db.insertOrThrow("entries", null, ContentValues().apply {
                put("id", "message"); put("bot_id", "bot"); put("thread_id", "root"); put("seq", 1); put("rev", 3)
                put("payload", WireJson.encodeToString(entry(3)))
            })
            db.insertOrThrow("pending", null, ContentValues().apply {
                put("nonce", "unsent"); put("created", 1)
                put("payload", """{"nonce":"unsent","botId":"bot","threadId":"root","text":"aside","createdAt":1}""")
            })
            db.insertOrThrow("history", null, ContentValues().apply { put("lane", "bot") })
            db.version = 2
        }
        mirror = MirrorDatabase(context, directory, computer)
        val snapshot = mirror.snapshot()
        assertTrue(snapshot.entries.isEmpty())
        assertEquals(0, snapshot.rev)
        assertFalse(mirror.historyComplete("bot"))
        assertEquals("aside", snapshot.pending.single().text)
        mirror.apply(MirrorEvent.EntryChanged(entry(4, nonce = "unsent")))
        assertTrue(mirror.snapshot().pending.isEmpty())
    }
}
