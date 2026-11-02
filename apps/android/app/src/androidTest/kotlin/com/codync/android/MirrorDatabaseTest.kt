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
        assertFalse(mirror.setStamp("old", "2.4/2.4"))
        mirror.apply(MirrorEvent.EntryChanged(entry(9)))
        mirror.save(PendingSend("unsent", "bot", text = "message", createdAt = 1))
        mirror.finishHistory("bot")
        assertFalse(mirror.setStamp("old", "2.4/2.4"))
        assertEquals(1, mirror.snapshot().entries.size)
        // The new host removed "message": the catch-up from 0 brings only what it still has.
        assertTrue(mirror.setStamp("old", "2.5/2.4"))
        assertEquals(0, mirror.snapshot().rev)
        assertFalse(mirror.historyComplete("bot"))
        assertEquals("unsent", mirror.snapshot().pending.single().nonce)
        mirror.apply(MirrorEvent.EntryChanged(Entry("kept", 2, "bot", rev = 10, kind = "user", data = buildJsonObject { put("text", "kept") })))
        assertEquals(listOf("kept"), mirror.snapshot().entries.map { it.id })
        assertTrue(mirror.setStamp("new", "2.5/2.4"))
        assertEquals(0, mirror.snapshot().rev)
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
