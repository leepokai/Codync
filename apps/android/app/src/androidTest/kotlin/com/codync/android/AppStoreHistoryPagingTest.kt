package com.codync.android

import android.app.Application
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.Bot
import com.codync.android.core.Computer
import com.codync.android.core.Entry
import com.codync.android.core.WorkStatus
import com.codync.android.core.WorkTask
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The store's own paging lifecycle around a chat a thread holds outside it. */
@RunWith(AndroidJUnit4::class)
class AppStoreHistoryPagingTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val directory = File(context.noBackupFilesDir, "store-paging-test-" + UUID.randomUUID()).apply { mkdirs() }
    private var store: AppStore? = null

    @After fun cleanup() {
        store?.let { held -> runBlocking(Dispatchers.Main) { held.retire(unregister = false) } }
        directory.deleteRecursively()
    }

    private fun chat(seq: Long) = Entry("m-$seq", seq, "bot", rev = 1, kind = "user",
        data = buildJsonObject { put("text", "message $seq") })

    @Test fun aClosedThreadReclaimsItsPagingStopAndReopeningPagesBackToTheApproval() = runBlocking(Dispatchers.Main) {
        val application = context.applicationContext as Application
        val computerId = "store-paging-test-0001"
        val opened = AppStore(application, LocalStore(application), Computer(computerId, "Paging test", "android"),
            "local", 0, SupervisorJob(), {}, {})
        store = opened
        val mirror = MirrorDatabase(context, directory, computerId)
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            putJsonObject("thread") { put("id", "thread-1") }
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
        })
        mirror.receive(listOf(approval) + (2L..101L).map(::chat))
        opened.database = mirror

        // The owner is unselected, held by the thread, and its previous page already reached the end.
        opened.mutableState.update { it.copy(loading = false, selectedBot = null, historyEnd = mapOf("bot" to HistoryEnd.Complete)) }
        opened.watchThread("bot", "thread-1")
        opened.refresh()
        assertFalse("the approval starts beyond the held owner's first window", opened.state.value.entries.any { it.id == "ask" })

        opened.closeThread("bot", "thread-1")
        opened.refresh()
        assertFalse("closing the last view reclaims the paging stop", "bot" in opened.state.value.historyEnd)
        assertFalse("closing the last view reclaims the widened window", opened.windows.containsKey("bot"))

        // Reopening the thread has to load from the first window again and page back to the card.
        opened.watchThread("bot", "thread-1")
        opened.refresh()
        val task = WorkTask("task", title = "Needs an answer", status = WorkStatus.NeedsYou, thread = "thread-1")
        opened.holdChat("bot", task)
        opened.actions[historyKey("bot")]?.join()
        assertFalse("the paging action finished", historyKey("bot") in opened.state.value.busy)
        assertTrue("the reopened thread pages back to its old approval", opened.state.value.entries.any { it.id == "ask" })
        assertTrue("the approval is the card its task waits on", pendingCard(opened.state.value.entries, task)?.id == "ask")
    }

    @Test fun anInFlightPageCannotRestoreAPagingStopAfterTheLastViewCloses() = runBlocking(Dispatchers.Main) {
        val application = context.applicationContext as Application
        val computerId = "store-paging-test-0002"
        val opened = AppStore(application, LocalStore(application), Computer(computerId, "Paging test", "android"),
            "local", 0, SupervisorJob(), {}, {})
        store = opened
        val mirror = MirrorDatabase(context, directory, computerId)
        mirror.receive((101L..120L).map(::chat))
        // The previous visit had already reached the end of the computer's history.
        mirror.finishHistory("bot")
        opened.database = mirror
        opened.mutableState.update { it.copy(loading = false, selectedBot = null) }
        opened.watchThread("bot", "thread-1")
        opened.refresh()

        val atCommit = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        opened.beforeHistoryCommit = { botId ->
            if (botId == "bot") {
                atCommit.complete(Unit)
                release.await()
            }
        }
        val task = WorkTask("task", title = "Needs an answer", status = WorkStatus.NeedsYou, thread = "thread-1")
        opened.holdChat("bot", task)
        atCommit.await()

        // The last view closes while the old page waits to record Complete.
        opened.closeThread("bot", "thread-1")
        opened.refresh()
        assertFalse("the last view still reclaimed its stop", "bot" in opened.state.value.historyEnd)
        release.complete(Unit)
        opened.actions[historyKey("bot")]?.join()
        assertFalse("the stale page cannot restore the stop", "bot" in opened.state.value.historyEnd)
        assertFalse("the stale page cannot restore its window", opened.windows.containsKey("bot"))
        opened.beforeHistoryCommit = null

        // Earlier rows arrive on the phone, and a new visit has to page to the approval those rows hold.
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            putJsonObject("thread") { put("id", "thread-1") }
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
        })
        mirror.receive(listOf(approval) + (2L..100L).map(::chat))
        opened.watchThread("bot", "thread-1")
        opened.refresh()
        assertFalse("the approval starts beyond the reopened first window", opened.state.value.entries.any { it.id == "ask" })
        opened.holdChat("bot", task)
        opened.actions[historyKey("bot")]?.join()
        assertTrue("the reopened thread pages back to its old approval", opened.state.value.entries.any { it.id == "ask" })
        assertTrue("the approval is the card its task waits on", pendingCard(opened.state.value.entries, task)?.id == "ask")
    }

    @Test fun aStaleCardPageCannotClearTheRequestOfTheVisitThatReplacedIt() = runBlocking(Dispatchers.Main) {
        val application = context.applicationContext as Application
        val computerId = "store-paging-test-0003"
        val opened = AppStore(application, LocalStore(application), Computer(computerId, "Paging test", "android"),
            "local", 0, SupervisorJob(), {}, {})
        store = opened
        val mirror = MirrorDatabase(context, directory, computerId)
        mirror.receive(listOf(Bot("bot", name = "Bot"), Bot("other", name = "Other")))
        mirror.receive((101L..120L).map(::chat))
        mirror.finishHistory("bot")
        opened.database = mirror
        opened.mutableState.update { it.copy(loading = false, selectedBot = "bot") }
        opened.refresh()

        val task = WorkTask("task", title = "Needs an answer", status = WorkStatus.NeedsYou, thread = "thread-1")
        opened.openCard("bot", task)
        val atCommit = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        opened.beforeHistoryCommit = { botId ->
            if (botId == "bot") {
                atCommit.complete(Unit)
                release.await()
            }
        }
        opened.pageToCard("bot", task)
        atCommit.await()

        // Leave the chat and reopen the SAME task before the old page's finally block can run.
        opened.openBot("other")
        opened.openCard("bot", task)
        val replacement = opened.state.value.cardRequest
        assertEquals("the replacement request is the same bot and task", "bot" to task, replacement)
        val replacementEpoch = opened.cardRequestEpoch.get()

        release.complete(Unit)
        opened.actions[historyKey("bot")]?.join()
        assertEquals("the old page cannot clear the replacement request", replacement, opened.state.value.cardRequest)
        assertEquals("the replacement request kept its identity", replacementEpoch, opened.cardRequestEpoch.get())
        opened.beforeHistoryCommit = null

        // The replacement visit can still find and keep its card, which is the retry the request exists for.
        val approval = Entry("ask", 1, "bot", rev = 1, kind = "permission", data = buildJsonObject {
            putJsonObject("thread") { put("id", "thread-1") }
            put("title", "Edit docs/design/bot-memory.md")
            put("status", "pending")
        })
        mirror.receive(listOf(approval))
        opened.pageToCard("bot", task)
        opened.actions[historyKey("bot")]?.join()
        assertTrue("the replacement visit pages to its approval", opened.state.value.entries.any { it.id == "ask" })
        assertEquals("the found card still waits for the chat to take its request", replacement, opened.state.value.cardRequest)
    }
}
