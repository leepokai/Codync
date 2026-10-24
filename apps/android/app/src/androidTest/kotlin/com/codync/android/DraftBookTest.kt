package com.codync.android

import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.After
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.UUID

class DraftBookTest {
    private val root = File(InstrumentationRegistry.getInstrumentation().targetContext.noBackupFilesDir,
        "draft-test-" + UUID.randomUUID()).apply { mkdirs() }
    private val computer = "AAAAAAAAAAAAAAAAAAAAAA"
    @After fun cleanup() { root.deleteRecursively() }

    @Test fun reopeningRestoresEachChatWithoutCrossingAccountsOrComputers() {
        val file = File(root, "outgoing/file").apply { requireNotNull(parentFile).mkdirs(); writeText("saved attachment") }
        val main = ComposerDraft("bot", text = "Unsent main chat", files = listOf(OutgoingFile(file.path, "file.txt", file.length())))
        val other = ComposerDraft("other", text = "Unsent elsewhere")
        DraftBook(root, computer).save(listOf(main, other))
        assertEquals(listOf(main, other), DraftBook(root, computer).read())
        // A book saved with thread drafts keeps each chat's own.
        File(root, "drafts-$computer.json").writeText("""[{"botId":"bot","threadId":"root","text":"Aside","nonce":"${other.nonce}"},
            {"botId":"bot","threadId":null,"text":"Main","nonce":"${main.nonce}"}]""")
        assertEquals(listOf("Main"), DraftBook(root, computer).read().map { it.text })
        assertTrue(DraftBook(root, "BBBBBBBBBBBBBBBBBBBBBB").read().isEmpty())
        val otherAccount = File(root, "other-account").apply { mkdirs() }
        assertTrue(DraftBook(otherAccount, computer).read().isEmpty())
        assertTrue(file.exists())
    }

    @Test fun aCrashAfterPendingCommitCannotRestoreAnotherSendForTheSameDraft() {
        val draft = ComposerDraft("bot", text = "Send once")
        val book = DraftBook(root, computer)
        book.save(listOf(draft))
        val pending = PendingSend(draft.nonce, "bot", text = draft.text, createdAt = 1)
        assertTrue(book.recover(MirrorSnapshot(pending = listOf(pending))).isEmpty())
        assertTrue(DraftBook(root, computer).read().isEmpty())
        book.save(listOf(draft))
        val confirmed = Entry("entry", 1, "bot", rev = 1, kind = "user",
            data = buildJsonObject { put("clientNonce", draft.nonce); put("text", draft.text) })
        assertTrue(book.recover(MirrorSnapshot(entries = listOf(confirmed))).isEmpty())
    }

    @Test fun rejectedOversizedOrForeignAttachmentWritesRetainThePreviousDraft() {
        val book = DraftBook(root, computer)
        val safe = ComposerDraft("bot", text = "Keep this")
        book.save(listOf(safe))
        for (invalid in listOf(safe.copy(text = "x".repeat(65537)),
            safe.copy(files = listOf(OutgoingFile(File(root, "identity").path, "private", 1))))) {
            try { book.save(listOf(invalid)); fail("Expected draft validation") }
            catch (_: IllegalArgumentException) { }
            assertEquals(listOf(safe), book.read())
        }
    }

    @Test fun clearingOneLaneKeepsOtherDraftsAndTheDraftCountIsBounded() {
        val book = DraftBook(root, computer)
        val drafts = (0 until 64).map { ComposerDraft("bot-$it", text = "Unsent $it") }
        book.save(drafts)
        try { book.save(drafts + ComposerDraft("overflow", text = "Unsent")); fail("Expected bounded drafts") }
        catch (_: IllegalArgumentException) { }
        assertEquals(drafts, book.read())
        book.save(drafts.drop(1))
        assertEquals(drafts.drop(1), book.read())
    }
}
