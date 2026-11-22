package com.codync.android.core

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class WorkTest {
    private fun bot(fields: String = "") = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"Bot","rev":1$fields}""").jsonObject)
    private fun work(json: String) = bot(""","work":$json""").work
    private fun task(id: String, status: WorkStatus) = WorkTask(id, status = status)

    @Test fun `a bot's work decodes, and survives the mirror's copy`() {
        val bot = bot(""","maxThreads":4,"work":[
            {"id":"w1","title":"Fix login","status":"working","activity":"Reading","thread":"thread-1","wait":null,"cutIn":false,"createdAt":3,"startedAt":5,"endedAt":null,"result":"","brief":"x"},
            {"id":"w2","title":"Docs","status":"waiting","thread":null,"wait":{"kind":"behind","ahead":2},"cutIn":true,"createdAt":4},
            {"id":"w3","title":"CI","status":"failed","thread":"thread-0","createdAt":1,"startedAt":2,"endedAt":9,"result":"No network"}]""")
        assertEquals(listOf(
            WorkTask("w1", "Fix login", WorkStatus.Working, "Reading", "thread-1", createdAt = 3, startedAt = 5),
            WorkTask("w2", "Docs", WorkStatus.Waiting, wait = WorkWait(WaitKind.Behind, 2), cutIn = true, createdAt = 4),
            WorkTask("w3", "CI", WorkStatus.Failed, thread = "thread-0", createdAt = 1, startedAt = 2, endedAt = 9, result = "No network")), bot.work)
        assertEquals(4, bot.maxThreads)
        assertEquals(bot, HostClient.decode(Json.parseToJsonElement(WireJson.encodeToString(Bot.serializer(), bot)).jsonObject))
    }

    @Test fun `a host from before the box has no work and no ceiling`() {
        assertEquals(emptyList<WorkTask>(), bot().work)
        assertNull(bot().maxThreads)
        assertNull(BotDraft.from(bot()).maxThreads)
        assertFalse("maxThreads" in BotDraft.from(bot()).body())
    }

    @Test fun `work this app can't read never loses the bot`() {
        assertEquals(emptyList<WorkTask>(), work("null"))
        assertEquals(emptyList<WorkTask>(), work(""""soon""""))
        assertEquals(emptyList<WorkTask>(), work("""{"id":"w1","status":"working"}"""))
        // A task without an id or in a status this app doesn't know is left out; one with fields missing or null still shows.
        val mixed = work("""[7, {"title":"No id","status":"working"}, {"id":"w1","status":"paused"}, {"id":"w2"},
            {"id":"w3","status":"done","title":null,"startedAt":null}, {"id":"w4","status":"waiting"}]""")
        assertEquals(listOf(task("w3", WorkStatus.Done), task("w4", WorkStatus.Waiting)), mixed)
    }

    @Test fun `a reason to wait this app can't read is none`() {
        fun wait(json: String) = work("""[{"id":"w1","status":"waiting","wait":$json}]""").single().wait
        assertEquals(WorkWait(WaitKind.Next), wait("""{"kind":"next"}"""))
        assertEquals(WorkWait(WaitKind.Behind, 1), wait("""{"kind":"behind"}"""))
        assertEquals(WorkWait(WaitKind.Usage), wait("""{"kind":"usage","ahead":null}"""))
        for (malformed in listOf("\"next\"", """{"kind":"soon"}""", "{}", "7", """{"kind":"behind","ahead":"two"}""")) assertNull(malformed, wait(malformed))
    }

    @Test fun `the ceiling stays within one to six, and one this app can't read is none`() {
        assertEquals(1, bot(""","maxThreads":0""").maxThreads)
        assertEquals(6, bot(""","maxThreads":9""").maxThreads)
        assertEquals(3, bot(""","maxThreads":3.0""").maxThreads)
        for (malformed in listOf("null", "\"3\"", "true", "[3]", "{}")) assertNull(malformed, bot(""","maxThreads":$malformed""").maxThreads)
    }

    @Test fun `the ceiling is saved with the bot's other settings`() {
        val draft = BotDraft.from(bot(""","maxThreads":3""")).copy(maxThreads = 5)
        assertEquals("5", draft.body().getValue("maxThreads").toString())
    }

    @Test fun `tasks fall into threads, the box and recent, in the order their rows show`() {
        val work = listOf(task("a", WorkStatus.Working), task("b", WorkStatus.NeedsYou), task("c", WorkStatus.Working), task("d", WorkStatus.Waiting),
            task("e", WorkStatus.Waiting), task("f", WorkStatus.Stopped), task("g", WorkStatus.Done), task("h", WorkStatus.Failed))
        val sections = workSections(work, 4)
        assertEquals(listOf("b", "a", "c"), sections.threads.map { it.id })
        assertEquals(listOf("d", "e"), sections.box.map { it.id })
        assertEquals(listOf("f", "g", "h"), sections.recent.map { it.id })
        // Three run of four: one place is free.
        assertEquals(1 to 4, sections.free to sections.atOnce)
        // More running than the ceiling (it was just lowered): none free, and the heading says what is true, `3 / 2`.
        assertEquals(0 to 2, workSections(work, 2).let { it.free to it.atOnce })
        // A host that sends no ceiling counts as three.
        assertEquals(WorkSections(emptyList(), 0, 3, emptyList(), emptyList()), workSections(emptyList(), null))
        assertEquals(0 to 3, workSections(work, null).let { it.free to it.atOnce })
    }

    @Test fun `a thread waiting on the user gives its seat up, as the host says`() {
        val work = work("""[{"id":"a","status":"working","seat":true},{"id":"b","status":"needsYou","seat":false},{"id":"c","status":"waiting","seat":false}]""")
        val sections = workSections(work, 2)
        assertEquals(Triple(2, 1, 1), Triple(sections.threads.size, sections.inUse, sections.free))
        // A seat is free: nothing has to give way.
        assertFalse(canCutIn(work, 2))
        // An older host says nothing: every live task takes one.
        val old = listOf(task("a", WorkStatus.Working), task("b", WorkStatus.NeedsYou), task("c", WorkStatus.Waiting))
        assertEquals(2, workSections(old, 2).inUse)
        assertTrue(canCutIn(old, 2))
    }

    @Test fun `only the tasks a thread has are live`() {
        assertEquals(listOf(WorkStatus.Working, WorkStatus.NeedsYou), WorkStatus.entries.filter { it.live })
        assertEquals(listOf(WorkStatus.Done, WorkStatus.Failed, WorkStatus.Stopped), WorkStatus.entries.filter { it.ended })
    }

    @Test fun `cutting in is for when every place is taken`() {
        val two = listOf(task("a", WorkStatus.Working), task("b", WorkStatus.NeedsYou), task("c", WorkStatus.Waiting), task("d", WorkStatus.Done))
        assertTrue(canCutIn(two, 2))
        assertFalse(canCutIn(two, 3))
        // A host that sends no ceiling counts as three.
        assertFalse(canCutIn(two, null))
        assertTrue(canCutIn(two + task("e", WorkStatus.Working), null))
        // A freed place wouldn't start one that follows unfinished work or waits for usage.
        for (kind in listOf(WaitKind.After, WaitKind.Usage)) {
            val held = task("c", WorkStatus.Waiting).copy(wait = WorkWait(kind))
            assertFalse(canCutIn(listOf(task("a", WorkStatus.Working), held), 1))
        }
    }

    @Test fun `a row offers what its task can take, the action that shows first`() {
        val work = listOf(task("a", WorkStatus.Working), task("b", WorkStatus.NeedsYou), task("c", WorkStatus.Waiting), task("d", WorkStatus.Waiting),
            task("e", WorkStatus.Done), task("f", WorkStatus.Failed), task("g", WorkStatus.Stopped))
        fun actions(id: String, maxThreads: Int? = 3) = workActions(work.first { it.id == id }, work, maxThreads)
        assertEquals(listOf(WorkAction.Stop), actions("a"))
        assertEquals(listOf(WorkAction.Stop), actions("b"))
        // The first in line can only be dropped, until every place is taken: then it can cut in.
        assertEquals(listOf(WorkAction.Drop), actions("c"))
        assertEquals(listOf(WorkAction.CutIn, WorkAction.Drop), actions("c", maxThreads = 2))
        assertEquals(listOf(WorkAction.First, WorkAction.Drop), actions("d", maxThreads = 2))
        assertEquals(listOf(WorkAction.Drop), actions("e"))
        assertEquals(listOf(WorkAction.Retry, WorkAction.Drop), actions("f"))
        assertEquals(listOf(WorkAction.Retry, WorkAction.Drop), actions("g"))
        assertEquals(listOf(WorkAction.Stop, WorkAction.Drop), WorkAction.entries.filter { it.destructive })
    }

    @Test fun `all bots' work lists the roster's bots with a live or waiting task, in roster order, and counts what is worked on`() {
        val roster = listOf(
            "idle" to emptyList(),
            "done" to listOf(task("a", WorkStatus.Done), task("b", WorkStatus.Stopped), task("c", WorkStatus.Failed)),
            "busy" to listOf(task("d", WorkStatus.Working), task("e", WorkStatus.Working), task("f", WorkStatus.Waiting), task("g", WorkStatus.Done)),
            "asks" to listOf(task("h", WorkStatus.NeedsYou)),
            "queued" to listOf(task("i", WorkStatus.Waiting)),
            "one" to listOf(task("j", WorkStatus.Working)),
        )
        val all = allWork(roster) { it.second }
        assertEquals(listOf("busy", "asks", "queued", "one"), all.bots.map { it.first })
        // Tasks that need the user or wait are not counted as working.
        assertEquals(3, all.working)
        // Nothing live or waiting: no button, and an open view closes.
        assertEquals(AllWork(emptyList<Pair<String, List<WorkTask>>>(), 0), allWork(roster.take(2)) { it.second })
        // The button shows without a count.
        assertEquals(0, allWork(listOf(roster[3], roster[4])) { it.second }.working)
        assertEquals(listOf(WorkStatus.Waiting, WorkStatus.Working, WorkStatus.NeedsYou), WorkStatus.entries.filter { it.open })
    }

    @Test fun `a task says where it came from, and a source this app doesn't know reads as the bot's own`() {
        val sources = work("""[{"id":"a","status":"working"}, {"id":"b","status":"working","source":{"kind":"routine","id":"r1"}},
            {"id":"c","status":"working","source":{"kind":"request","id":"bot-2"}}, {"id":"d","status":"working","source":{"kind":"request"}},
            {"id":"e","status":"working","source":{"kind":"webhook","id":"x"}}, {"id":"f","status":"working","source":"routine"},
            {"id":"g","status":"working","source":{"kind":"request","id":null}}]""").map { it.source }
        assertEquals(listOf(null, WorkSource(SourceKind.Routine, "r1"), WorkSource(SourceKind.Request, "bot-2"), WorkSource(SourceKind.Request), null, null,
            WorkSource(SourceKind.Request)), sources)
    }

    @Test fun `a routine's run or another bot's request is retried and set aside by its own rules`() {
        val routine = WorkSource(SourceKind.Routine, "r1")
        val request = WorkSource(SourceKind.Request, "bot-2")
        assertTrue(canRetry(task("a", WorkStatus.Failed)))
        assertTrue(canRetry(task("a", WorkStatus.Stopped)))
        // It runs the routine again.
        assertTrue(canRetry(task("a", WorkStatus.Stopped).copy(source = routine)))
        assertFalse(canRetry(task("a", WorkStatus.Failed).copy(source = request)))
        assertFalse(canRetry(task("a", WorkStatus.Stopped).copy(source = WorkSource(SourceKind.Request))))
        for (status in listOf(WorkStatus.Waiting, WorkStatus.Working, WorkStatus.NeedsYou, WorkStatus.Done)) assertFalse(status.name, canRetry(task("a", status)))
        assertEquals(listOf(WorkAction.Drop), workActions(task("a", WorkStatus.Failed).copy(source = request), emptyList(), 3))

        assertTrue(givesWay(task("a", WorkStatus.Working)))
        assertTrue(givesWay(task("a", WorkStatus.NeedsYou)))
        assertFalse(givesWay(task("a", WorkStatus.Waiting)))
        assertFalse(givesWay(task("a", WorkStatus.Working).copy(source = routine)))
        assertFalse(givesWay(task("a", WorkStatus.NeedsYou).copy(source = request)))

        val waiting = task("w", WorkStatus.Waiting)
        assertFalse(canCutIn(listOf(task("a", WorkStatus.Working).copy(source = routine), task("b", WorkStatus.Working).copy(source = request), waiting), 2))
        assertTrue(canCutIn(listOf(task("a", WorkStatus.Working).copy(source = routine), task("b", WorkStatus.Working), waiting), 2))
        // A routine task may cut in itself.
        assertTrue(canCutIn(listOf(task("a", WorkStatus.Working), waiting.copy(source = routine)), 1))
    }

    @Test fun `a worktree per agent is the bot's setting, shown where its folder is a git repository`() {
        val bot = bot(""","threadWorktrees":true,"gitRepo":true,"work":[
            {"id":"a","status":"working","worktree":{"branch":"codync/fix-login","path":"/w/a"}}, {"id":"b","status":"working"},
            {"id":"c","status":"working","worktree":"codync/x"}, {"id":"d","status":"working","worktree":{"path":"/w/d"}},
            {"id":"e","status":"working","worktree":{"branch":"codync/e","path":null}}]""")
        assertEquals(true, bot.threadWorktrees)
        assertTrue(bot.gitRepo)
        // A worktree this app can't read (no branch) is none; the task still shows.
        assertEquals(listOf(Worktree("codync/fix-login", "/w/a"), null, null, null, Worktree("codync/e")), bot.work.map { it.worktree })
        assertEquals("false", BotDraft.from(bot).copy(threadWorktrees = false).body().getValue("threadWorktrees").toString())
        // An older host has neither: no switch, and a save leaves it out.
        assertNull(bot().threadWorktrees)
        assertFalse(bot().gitRepo)
        assertFalse("threadWorktrees" in BotDraft.from(bot()).body())
    }

    @Test fun `a bot stands outside its chat as the host says, else as its chat`() {
        val busy = bot(""","status":"idle","startedAt":4,"outside":{"status":"working","startedAt":9}""")
        assertEquals(Outside("working", 9), busy.outside)
        assertEquals("working" to 9L, busy.outsideStatus to busy.outsideStartedAt)
        assertEquals(Outside("needsInput"), bot(""","outside":{"status":"needsInput","startedAt":null}""").outside)
        // An older host, or an `outside` this app can't read: the chat's own.
        for (absent in listOf("", ""","outside":null""", ""","outside":"working"""", ""","outside":{}""", ""","outside":{"status":7}""", ""","outside":[1]""")) {
            val old = bot(""","status":"working","startedAt":4$absent""")
            assertNull(absent, old.outside)
            assertEquals(absent, "working" to 4L, old.outsideStatus to old.outsideStartedAt)
        }
        assertEquals(busy, HostClient.decode(Json.parseToJsonElement(WireJson.encodeToString(Bot.serializer(), busy)).jsonObject))
        // The widgets read it too: a bot whose tasks work after its chat's turn reads as working.
        assertEquals(listOf("working"), widgetBots(mapOf("c" to listOf(busy))).map { it.status })
    }

    @Test fun `the host says which tasks take a message`() {
        val tasks = work("""[{"id":"a","status":"working","thread":"t","talk":true,"talkFiles":true,"folder":"/Users/kai/code/app"},
            {"id":"b","status":"working","thread":"t2","talk":false,"source":{"kind":"routine","id":"r"}}, {"id":"c","status":"waiting","folder":null}]""")
        assertEquals(listOf(true, false, false), tasks.map { it.talk })
        assertEquals(listOf(true, false, false), tasks.map { it.talkFiles })
        assertEquals(listOf("/Users/kai/code/app", null, null), tasks.map { it.folder })
    }

    @Test fun `a task says where it works, the branch for one in a worktree of its own`() {
        val home = "/Users/kai"
        val folder = WorkTask("a", status = WorkStatus.Working, folder = "/Users/kai/code/app")
        assertEquals(WhereLine(WhereKind.Folder, "~/code/app", "/Users/kai/code/app"), whereLine(folder, home))
        val tree = folder.copy(worktree = Worktree("codync/fix-login", "/w/a"), folder = "/w/a/server")
        assertEquals(WhereLine(WhereKind.Branch, "codync/fix-login", "/w/a/server"), whereLine(tree, home))
        // An older host names the worktree alone: its top is what copies; one with no path copies nothing.
        assertEquals("/w/a", whereLine(tree.copy(folder = null), home)?.path)
        assertNull(whereLine(tree.copy(folder = null, worktree = Worktree("codync/x")), home)?.path)
        assertNull(whereLine(task("w", WorkStatus.Waiting), home))
        assertEquals("folder" to "arrow.triangle.branch", WhereKind.Folder.symbol to WhereKind.Branch.symbol)
    }

    @Test fun `a folder starts at the computer's home`() {
        assertEquals("~", homeRelative("/Users/kai", "/Users/kai/"))
        assertEquals("~/code/app", homeRelative("/home/kai/code/app", "/home/kai"))
        assertEquals("~/app", homeRelative("/root/app", "/root"))
        assertEquals("""~\code\app""", homeRelative("""C:\Users\kai\code\app""", """C:\Users\kai"""))
        // Another folder that only starts the same, anywhere else, and a computer that names no home: as it is.
        assertEquals("/Users/kaito/site", homeRelative("/Users/kaito/site", "/Users/kai"))
        assertEquals("/Users/Shared/site", homeRelative("/Users/Shared/site", "/Users/kai"))
        assertEquals("/var/www/site", homeRelative("/var/www/site", "/Users/kai"))
        assertEquals("/tmp", homeRelative("/tmp", ""))
    }

    @Test fun `a thread's draft goes with its bot`() {
        assertEquals("b/item/i", threadDraftKey("b", "i"))
        assertTrue(threadDraftKey("b", "i").startsWith("b/"))
    }

    @Test fun `a thread shows what it put in the chat where it came`() {
        fun entry(id: String, at: Long, thread: String? = null, botId: String = "b") = Entry(id, 1, botId, 1, "agent", createdAt = at,
            data = JsonObject(thread?.let { mapOf("thread" to JsonObject(mapOf("id" to JsonPrimitive(it)))) } ?: emptyMap()))
        val own = listOf(entry("t1", 10, botId = "thread-1"), entry("t2", 30, botId = "thread-1"))
        val chat = listOf(entry("c1", 5, "thread-1"), entry("c2", 20, "thread-1"), entry("c3", 21), entry("c4", 30, "thread-1"), entry("c5", 31, "thread-2"))
        // The thread's own first at the same moment; the chat's own and another thread's stay in the chat.
        assertEquals(listOf("c1", "t1", "c2", "t2", "c4"), sharing(own, chat, "thread-1").map { it.id })
        assertEquals(listOf("t1", "t2"), sharing(own, chat, "thread-9").map { it.id })
    }

    @Test fun `what a thread put in the chat is one line of its thread, saying how it stands`() {
        fun line(kind: String, data: String) = sharedLine(Entry("c", 1, "b", 1, kind, data = Json.parseToJsonElement(data).jsonObject)) { it }
        val options = """"options":[{"optionId":"ok","name":"Allow","kind":"allow_once"},{"optionId":"no","name":"Deny","kind":"reject_once"},{"optionId":"x","name":"Skip","kind":"other"}]"""
        fun approval(status: String, selected: String = "") = line("permission", """{"status":"$status","title":"npm test",$options,"selected":"$selected"}""")
        assertEquals(SharedLine("hand.raised", "Waiting for your approval: npm test"), approval("pending"))
        assertEquals(SharedLine("checkmark", "Approved: npm test"), approval("answered", "ok"))
        assertEquals(SharedLine("xmark", "Denied: npm test"), approval("answered", "no"))
        assertEquals("Answered: Skip", approval("answered", "x")?.text)
        assertEquals(SharedLine("clock", "Not answered: npm test"), approval("expired"))
        fun choice(status: String, selected: String = "") = line("notice", """{"decision":true,"status":"$status","question":"Squash?","choices":["Yes","No"],"selected":"$selected"}""")
        assertEquals("Asked: Squash?", choice("pending")?.text)
        assertEquals("Answered: No", choice("answered", "No")?.text)
        assertEquals("Not answered: Squash?", choice("expired")?.text)
        fun signIn(status: String, kind: String = "agent") = line("notice", """{"text":"Needs you to sign in.","connectionRequest":{"kind":"$kind","status":"$status","title":"Kimi Code"}}""")
        assertEquals(SharedLine("hand.raised", "Needs Kimi Code sign-in"), signIn("pending"))
        assertEquals(SharedLine("checkmark", "Signed in"), signIn("ready"))
        assertEquals(SharedLine("xmark", "Cancelled"), signIn("cancelled"))
        assertNull("a connector's card is not a thread's", line("notice", """{"connectionRequest":{"kind":"oauth","status":"pending","title":"Linear"}}"""))
        val file = """{"id":"f","name":"log.txt","size":3,"sha256":""}"""
        assertEquals(SharedLine("doc", "Sent a file: log.txt"), line("agent", """{"final":true,"files":[$file]}"""))
        assertEquals(SharedLine("waveform", "Sent a voice note"), line("agent", """{"final":true,"files":[$file],"voiceNote":true,"text":"Hi"}"""))
        assertEquals(SharedLine("doc.richtext", "Made an artifact: Plan"), line("agent", """{"final":true,"artifact":{"id":"a","title":"Plan","updatedAt":0}}"""))
        assertNull(line("agent", """{"text":"Just words","final":true}"""))
        assertNull(line("notice", """{"text":"A notice"}"""))
    }

    @Test fun `a bot's folder is a repository, isn't one, or the computer has no git`() {
        assertEquals(GitState.Repo, gitState(bot(""","threadWorktrees":true,"gitRepo":true,"gitInstalled":true""")))
        assertEquals(GitState.None, gitState(bot(""","threadWorktrees":true,"gitRepo":false,"gitInstalled":true""")))
        assertEquals(GitState.Missing, gitState(bot(""","threadWorktrees":true,"gitInstalled":false""")))
        // A host that doesn't say whether git is installed has it; one from before worktrees says nothing.
        assertEquals(GitState.None, gitState(bot(""","threadWorktrees":false""")))
        assertEquals(GitState.Repo, gitState(bot(""","threadWorktrees":false,"gitRepo":true,"gitInstalled":null""")))
        assertNull(gitState(bot()))
        assertEquals("Git isn't installed on this computer", GitState.Missing.title)
    }

    @Test fun `a thread's transcript is what the thread was told first, then the rest without what was taken back`() {
        fun entry(id: String, kind: String, data: String = "{}") =
            Entry(id, id.drop(1).toLong(), "thread-1", 1, kind, data = Json.parseToJsonElement(data).jsonObject)
        val told = entry("e1", "user", """{"text":"Fix login","from":"host"}""")
        val tool = entry("e2", "tool")
        val mine = entry("e3", "user", """{"text":"Also the tests","status":"queued"}""")
        val removed = entry("e4", "user", """{"text":"Never mind","status":"cancelled","removed":true}""")
        // What the host told it later (a session that starts again) stays in its place.
        val again = entry("e5", "user", """{"text":"Pick it up again","from":"host"}""")
        assertEquals(ThreadTranscript(told, listOf(tool, mine, again)), threadTranscript(listOf(told, tool, mine, removed, again)))
        assertEquals(listOf(true, false, false, true), listOf(told, tool, mine, again).map { it.isToldToThread })
        assertEquals(ThreadTranscript(null, listOf(tool)), threadTranscript(listOf(tool)))
        assertEquals(ThreadTranscript(null, emptyList()), threadTranscript(emptyList()))
    }

    @Test fun `what the chat passed on is quoted in the thread, never queued over its composer`() {
        fun entry(id: String, data: String) =
            Entry(id, id.drop(1).toLong(), "thread-1", 1, "user", data = Json.parseToJsonElement(data).jsonObject)
        val told = entry("e1", """{"text":"Fix login","from":"host"}""")
        val mine = entry("e2", """{"text":"Also the docs","status":"queued"}""")
        val passed = entry("e3", """{"text":"And the tests","from":"host","relay":true,"status":"queued"}""")
        val thread = threadTranscript(listOf(told, mine, passed))
        assertEquals(listOf(mine), thread.queue(working = true))
        assertEquals(listOf(passed), thread.hidingQueue(working = true))
        assertEquals(emptyList<Entry>(), thread.queue(working = false))
        assertEquals(listOf(mine, passed), thread.hidingQueue(working = false))
    }
}
