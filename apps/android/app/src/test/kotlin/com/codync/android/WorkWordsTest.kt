package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.Computer
import com.codync.android.core.Entry
import com.codync.android.core.MirrorEvent
import com.codync.android.core.PendingSend
import com.codync.android.core.SendStatus
import com.codync.android.core.SourceKind
import com.codync.android.core.WaitKind
import com.codync.android.core.WorkTask
import com.codync.android.core.WorkSource
import com.codync.android.core.WorkStatus
import com.codync.android.core.WorkWait
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class WorkWordsTest {
    private var ids = 0
    private fun task(status: WorkStatus, activity: String = "", title: String = "Fix login", wait: WorkWait? = null, cutIn: Boolean = false,
        result: String = "", thread: String? = null) =
        WorkTask("w${++ids}", title, status, activity, thread, wait, cutIn, createdAt = 1, startedAt = 5.takeIf { status != WorkStatus.Waiting }?.toLong(), result = result)
    private val working get() = task(WorkStatus.Working)
    private val waiting get() = task(WorkStatus.Waiting)
    private val needsYou get() = task(WorkStatus.NeedsYou)
    private val english = { text: String -> text }
    private val chinese = mapOf("Needs you" to "需要你", "{count} working" to "{count} 進行中", "{count} waiting" to "{count} 等待中", "Thinking…" to "思考中…",
        "Next up" to "下一個", "{count} ahead" to "前面還有 {count} 件", "Follows what it works on" to "接在進行中的事後面",
        "Near the usage limit: one at a time" to "用量接近上限，一次只跑一個",
        "Was cut in on; starts again" to "被插隊，會重新開始", "Waiting" to "等待中", "Stopped" to "已停止")
    private val traditional = { text: String -> chinese[text] ?: text }
    private fun bot(status: String = "idle", activity: String = "", work: List<WorkTask> = emptyList()) =
        Bot(id = "b", name = "Bot", status = status, activity = activity, work = work, lastMessage = "Hello")

    @Test fun `no capsule without a task live or waiting`() {
        assertNull(capsuleLabel(emptyList(), english))
        assertNull(capsuleLabel(listOf(task(WorkStatus.Done), task(WorkStatus.Failed), task(WorkStatus.Stopped)), english))
    }

    @Test fun `one working task and nothing waiting says what it does, with its time and its Stop`() {
        val thinking = task(WorkStatus.Working, "Thinking…")
        assertEquals(CapsuleLabel(WorkStatus.Working, "Thinking…", since = 5, stops = thinking), capsuleLabel(listOf(thinking, task(WorkStatus.Done)), english))
        // With no step yet, its title, which is the bot's own words.
        val quiet = working
        assertEquals(CapsuleLabel(WorkStatus.Working, "Fix login", since = 5, stops = quiet), capsuleLabel(listOf(quiet), traditional))
        assertEquals("思考中…", capsuleLabel(listOf(thinking), traditional)?.text)
    }

    @Test fun `the capsule counts what works and what waits, a part left out at none`() {
        fun text(vararg work: WorkTask) = capsuleLabel(work.toList(), english)?.text
        assertEquals("2 working", text(working, working))
        assertEquals("2 working · 3 waiting", text(working, working, waiting, waiting, waiting))
        assertEquals("1 working · 1 waiting", text(working, waiting))
        assertEquals("2 waiting", text(waiting, waiting))
        assertEquals("1 waiting", text(waiting, task(WorkStatus.Done)))
        assertEquals("2 進行中 · 1 等待中", capsuleLabel(listOf(working, working, waiting), traditional)?.text)
    }

    @Test fun `needs you leads the capsule, with the counts after it`() {
        fun label(vararg work: WorkTask) = capsuleLabel(work.toList(), english)
        assertEquals("Needs you", label(needsYou)?.text)
        assertEquals("Needs you · 2 working · 3 waiting", label(working, needsYou, working, waiting, waiting, waiting)?.text)
        assertEquals("Needs you · 1 working", label(needsYou, working)?.text)
        assertEquals("Needs you · 2 waiting", label(needsYou, waiting, waiting)?.text)
        assertEquals(WorkStatus.NeedsYou, label(needsYou, working)?.mark)
        assertEquals(WorkStatus.Working, label(working, waiting)?.mark)
        assertEquals(WorkStatus.Waiting, label(waiting, waiting)?.mark)
        assertNull(label(needsYou)?.since)
        assertEquals("需要你 · 1 等待中", capsuleLabel(listOf(needsYou, waiting), traditional)?.text)
    }

    @Test fun `the capsule's Stop is for the one task a thread has`() {
        val one = needsYou
        assertEquals(one, capsuleLabel(listOf(one, waiting), english)?.stops)
        val busy = working
        assertEquals(busy, capsuleLabel(listOf(busy, waiting, waiting), english)?.stops)
        assertNull(capsuleLabel(listOf(working, working), english)?.stops)
        assertNull(capsuleLabel(listOf(working, needsYou), english)?.stops)
        assertNull(capsuleLabel(listOf(waiting), english)?.stops)
        // It counts instead of timing once something waits behind the one.
        assertNull(capsuleLabel(listOf(busy, waiting), english)?.since)
    }

    @Test fun `a roster row says the most pressing thing first`() {
        val box = listOf(working, working, waiting, waiting, waiting)
        // Needs you: the chat's own card says what it asks; a task's only that it needs the user, as on every client.
        assertEquals(BotLine(BotLineKind.NeedsYou, "Wants to run a command"), botLine(bot("needsInput", "Wants to run a command", box + needsYou), english))
        assertEquals(BotLine(BotLineKind.NeedsYou, "Needs approval"), botLine(bot("needsInput"), english))
        assertEquals(BotLine(BotLineKind.NeedsYou, "Needs you"), botLine(bot("working", "Thinking…", box + needsYou), english))
        // Then the chat's own activity.
        assertEquals(BotLine(BotLineKind.Working, "Thinking…"), botLine(bot("working", "Thinking…", box), english))
        assertEquals(BotLine(BotLineKind.Working, "Working…"), botLine(bot("working"), english))
        // Then how many tasks work and wait, either part alone when the other is none.
        assertEquals(BotLine(BotLineKind.Working, "2 working · 3 waiting"), botLine(bot(work = box), english))
        assertEquals(BotLine(BotLineKind.Working, "2 working"), botLine(bot(work = box.take(2)), english))
        assertEquals(BotLine(BotLineKind.Waiting, "3 waiting"), botLine(bot(work = box.drop(2)), english))
        assertEquals(BotLine(BotLineKind.Waiting, "3 等待中"), botLine(bot("error", work = box.drop(2)), traditional))
        // Nothing going on: the row reads as usual, its last message.
        assertNull(botLine(bot(), english))
        assertNull(botLine(bot(work = listOf(task(WorkStatus.Done), task(WorkStatus.Stopped))), english))
    }

    @Test fun `a waiting task says why it waits`() {
        fun text(wait: WorkWait?, cutIn: Boolean = false, tr: (String) -> String = english) = waitText(task(WorkStatus.Waiting, wait = wait, cutIn = cutIn), tr)
        assertEquals("Next up", text(WorkWait(WaitKind.Next)))
        assertEquals("2 ahead", text(WorkWait(WaitKind.Behind, 2)))
        assertEquals("Follows what it works on", text(WorkWait(WaitKind.After)))
        assertEquals("Near the usage limit: one at a time", text(WorkWait(WaitKind.Usage)))
        assertEquals("Waiting", text(null))
        // One that gave way says so, whatever it waits for now.
        assertEquals("Was cut in on; starts again", text(WorkWait(WaitKind.Next), cutIn = true))
        assertEquals(listOf("下一個", "前面還有 3 件", "接在進行中的事後面", "用量接近上限，一次只跑一個", "等待中", "被插隊，會重新開始"),
            listOf(text(WorkWait(WaitKind.Next), tr = traditional), text(WorkWait(WaitKind.Behind, 3), tr = traditional), text(WorkWait(WaitKind.After), tr = traditional),
                text(WorkWait(WaitKind.Usage), tr = traditional), text(null, tr = traditional), text(null, cutIn = true, tr = traditional)))
    }

    @Test fun `a thread that takes no message says whose it is, and the bot's own says nothing`() {
        val own = task(WorkStatus.Working)
        assertEquals("Read only: a routine's run", readOnlyLine(own.copy(source = WorkSource(SourceKind.Routine, "r1"))))
        assertEquals("Read only: another bot's request", readOnlyLine(own.copy(source = WorkSource(SourceKind.Request))))
        assertNull(readOnlyLine(own))
    }

    @Test fun `a message to a thread shows until its transcript has it, and a refused one keeps its place and why`() {
        fun send(nonce: String, status: SendStatus = SendStatus.Sending, error: String? = null) = PendingSend(nonce, "b", "hi", 1, status, error)
        val state = AppState(Computer("fixture", "Studio", "fixture"))
        val sent = state.holdingThreadSend("w1", send("n1")).holdingThreadSend("w1", send("n2"))
        val refused = sent.holdingThreadSend("w1", send("n1", SendStatus.Failed, "\"Fix login\" is a routine's run"))
        assertEquals(listOf("n1" to SendStatus.Failed, "n2" to SendStatus.Sending), refused.threadSends.getValue("w1").map { it.nonce to it.status })
        assertEquals(emptyMap<String, List<PendingSend>>(), refused.withoutThreadSend("w1", "n1").withoutThreadSend("w1", "n2").threadSends)
        // What `send` answers joins the thread's transcript only while that is open.
        val entry = Entry("e1", 1, "thread-1", 2, "user", data = buildJsonObject { put("text", "hi"); put("clientNonce", "n1") })
        assertEquals(emptyMap<String, List<Entry>>(), state.receivingThreadEntry(entry).threadEntries)
        assertEquals(mapOf("thread-1" to listOf(entry)), state.copy(threadEntries = mapOf("thread-1" to emptyList())).receivingThreadEntry(entry).threadEntries)
    }

    @Test fun `a row's line is why it waits, what its thread does or how it ended`() {
        assertEquals("Next up", taskLine(task(WorkStatus.Waiting, wait = WorkWait(WaitKind.Next)), english))
        assertEquals("思考中…", taskLine(task(WorkStatus.Working, "Thinking…"), traditional))
        assertEquals("Working", taskLine(working, english))
        assertEquals("需要你", taskLine(needsYou, traditional))
        assertEquals("Done", taskLine(task(WorkStatus.Done, result = "All tests pass"), english))
        // Why it failed is the thread's own words; without any, the status.
        assertEquals("No network", taskLine(task(WorkStatus.Failed, result = "No network"), traditional))
        assertEquals("已停止", taskLine(task(WorkStatus.Stopped), traditional))
    }

    @Test fun `a needs-you task's card is the newest pending one that names it or its thread`() {
        fun card(id: String, status: String = "pending", task: String? = null, thread: String? = null, kind: String = "permission") =
            Entry(id, 1, "b", 1, kind, data = buildJsonObject {
                put("status", status)
                task?.let { put("task", it) }
                thread?.let { putJsonObject("thread") { put("id", it); put("title", "Fix login") } }
            })
        val asking = task(WorkStatus.NeedsYou, thread = "thread-1")
        val entries = listOf(card("old", task = asking.id), card("answered", "answered", task = asking.id), card("other", task = "w-other", thread = "thread-2"),
            card("chat"), card("message", task = asking.id, kind = "agent"))
        assertEquals("old", pendingCard(entries, asking)?.id)
        assertEquals("new", pendingCard(entries + card("new", task = asking.id), asking)?.id)
        assertEquals("byThread", pendingCard(listOf(card("byThread", thread = "thread-1")), asking)?.id)
        assertNull(pendingCard(listOf(card("chat"), card("byThread", thread = "thread-1")), asking.copy(thread = null)))
        assertNull(pendingCard(emptyList(), asking))
        // A thread's choice card counts as its approval card does; a file or message it sent doesn't.
        fun choice(id: String, status: String = "pending") = Entry(id, 1, "b", 1, "notice", data = buildJsonObject {
            put("decision", true); put("status", status); put("question", "Which branch?"); put("task", asking.id)
        })
        assertEquals("choice", pendingCard(entries + choice("choice"), asking)?.id)
        assertEquals("old", pendingCard(entries + choice("answered-choice", "answered"), asking)?.id)
        assertEquals("Which branch?", cardLine(choice("choice"), english))
        // A sign-in the thread's agent needs, in the chat: the row says which agent.
        fun signIn(id: String, status: String = "pending") = Entry(id, 1, "b", 1, "notice", data = buildJsonObject {
            put("task", asking.id)
            putJsonObject("connectionRequest") { put("kind", "agent"); put("status", status); put("title", "Kimi Code") }
        })
        assertEquals("signIn", pendingCard(entries + signIn("signIn"), asking)?.id)
        assertEquals("old", pendingCard(entries + signIn("signedIn", "ready"), asking)?.id)
        assertEquals("Needs Kimi Code sign-in", cardLine(signIn("s"), english))
        // The turn left Kimi Code signed out, and its fallback was too: Kimi Code's card holds the task's message.
        fun holding(id: String, agent: String, held: List<String>) = Entry(id, 1, "b", 1, "notice", data = buildJsonObject {
            put("task", asking.id)
            putJsonObject("connectionRequest") {
                put("kind", "agent"); put("status", "pending"); put("title", agent)
                putJsonArray("entryIds") { held.forEach { add(it) } }
            }
        })
        assertEquals("kimi", pendingCard(entries + holding("kimi", "Kimi Code", listOf("m9")) + holding("codex", "Codex", emptyList()), asking)?.id)
        // An approval newer than the holding sign-in is what the running turn waits on.
        assertEquals("approval", pendingCard(entries + holding("kimi", "Kimi Code", listOf("m9")) + card("approval", task = asking.id), asking)?.id)
        // A card is a thread's when it names its task, by the thread's title or by the task's id (as on the iPhone); the chat's own names neither.
        assertTrue(choice("c").isFromThread)
        assertTrue(card("t", thread = "thread-1").isFromThread)
        assertFalse(card("chat").isFromThread)
    }

    @Test fun `a needs-you row says what its card asks`() {
        fun card(title: String?) = Entry("c", 1, "b", 1, "permission", data = buildJsonObject { title?.let { put("title", it) } })
        assertEquals("npm test", cardLine(card("npm test"), english))
        assertNull(cardLine(card(""), english))
        assertNull(cardLine(card(null), english))
        assertNull(cardLine(null, english))
    }

    @Test fun `the cut-in dialog names each live task, what it does and how long`() {
        assertEquals("Fix login · 思考中… · 1:05", cutInChoice(task(WorkStatus.Working, "Thinking…"), now = 65_005, traditional))
        assertEquals("Fix login · 需要你 · 0:10", cutInChoice(task(WorkStatus.NeedsYou, "Thinking…"), now = 10_005, traditional))
        assertEquals("Fix login · 0:00", cutInChoice(working, now = 5, english))
    }

    @Test fun `add a fallback opens the bot's Settings, where its agent is`() {
        assertEquals(DetailsRequest(DetailsTab.Settings, botId = "b"), DetailsRequest.fallbacks("b"))
        // The title and the capsule open what the bot does.
        assertEquals(DetailsTab.Details, DetailsRequest().tab)
    }

    @Test fun `a thread's entry never makes a chat unread`() {
        val entry = Entry("e", 1, "thread-1", 1, "user", data = buildJsonObject { put("text", "hi") })
        assertEquals(emptyList<String>(), readAfter(listOf(MirrorEvent.ThreadEntryChanged("thread-1", entry)), listOf("thread-1", "bot")))
    }

    @Test fun `the timer counts minutes, then hours`() {
        assertEquals("0:07", timerText(7_900))
        assertEquals("12:30", timerText(750_000))
        assertEquals("1:02:03", timerText(3_723_000))
        assertEquals("0:00", timerText(-5_000))
    }

    @Test fun `outside the chat a bot says what needs you, then its chat's activity, then how many work and wait`() {
        val box = listOf(working, waiting, waiting)
        assertEquals("Needs you", outsideLine(bot("working", "Thinking…", box + needsYou), english))
        assertEquals("需要你", outsideLine(bot(work = listOf(needsYou)), traditional))
        assertEquals("Thinking…", outsideLine(bot("working", "Thinking…", box), english))
        assertEquals("Wants to run a command", outsideLine(bot("needsInput", "Wants to run a command", box), english))
        assertEquals("1 working · 2 waiting", outsideLine(bot(work = box), english))
        // The chat works with nothing to say: its work is what there is to say.
        assertEquals("1 working · 2 waiting", outsideLine(bot("working", work = box), english))
        assertEquals("", outsideLine(bot(), english))
    }

    @Test fun `the ongoing notification follows the bot outside its chat, and ends once nothing is left`() {
        fun at(rev: Long, chat: String, outside: String? = null, startedAt: Long? = null) =
            Bot(id = "b", name = "Bot", status = chat, rev = rev, outside = outside?.let { com.codync.android.core.Outside(it, startedAt) })
        val watch = TaskWatch(botId = "b", name = "Bot", lastRev = 1, startedAt = 100)
        // The chat's turn ended while tasks work: it goes on, with the line and the start of what goes on.
        val going = watch.after(at(2, "idle", "working", startedAt = 250), "2 working", now = 1_000)!!
        assertEquals(listOf("working", false, "2 working", true, 2L, 250L, 901_000L),
            listOf(going.status, going.finished, going.activity, going.seenRunning, going.lastRev, going.startedAt, going.deadline))
        // A task waits on the user.
        assertEquals("needsInput" to false, going.after(at(3, "idle", "needsInput"), "Needs you")!!.let { it.status to it.finished })
        // One task ended, others go on: no end.
        assertEquals(false, going.after(at(3, "idle", "working"), "1 working")!!.finished)
        // The box is empty and the chat idle: it ends, once, keeping when it started.
        val ended = going.after(at(4, "idle", "idle"), "")!!
        assertEquals(listOf("idle", true, 250L), listOf(ended.status, ended.finished, ended.startedAt))
        assertNull(ended.after(at(5, "working", "working"), ""))
        assertEquals("error" to true, going.after(at(4, "error", "error"), "")!!.let { it.status to it.finished })
        // Nothing new, a state it doesn't show, or idle before it ever ran: nothing to post.
        assertNull(going.after(at(2, "idle", "idle"), ""))
        assertNull(going.after(at(3, "idle", "paused"), ""))
        assertNull(watch.after(at(2, "idle", "idle"), ""))
        // An older host names no outside: the chat's own status.
        assertEquals("working" to false, watch.after(at(2, "working"), "Thinking…")!!.let { it.status to it.finished })
        assertEquals(true, going.after(at(3, "idle"), "")!!.finished)
    }
}
