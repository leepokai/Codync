package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.WireJson
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HistoryPagingTest {
    @Test fun `paging waits until the earlier row is on screen and a page would move the chat`() {
        assertFalse(pageEarlier(settled = false, visible = true, loading = false, shown = 10, floor = 40, askedShown = -1, askedFloor = Long.MAX_VALUE))
        assertFalse(pageEarlier(settled = true, visible = false, loading = false, shown = 10, floor = 40, askedShown = -1, askedFloor = Long.MAX_VALUE))
        assertFalse(pageEarlier(settled = true, visible = true, loading = true, shown = 10, floor = 40, askedShown = -1, askedFloor = Long.MAX_VALUE))
        assertTrue(pageEarlier(settled = true, visible = true, loading = false, shown = 10, floor = 40, askedShown = -1, askedFloor = Long.MAX_VALUE))
        assertFalse(pageEarlier(settled = true, visible = true, loading = false, shown = 10, floor = 40, askedShown = 10, askedFloor = 40))
        assertTrue(pageEarlier(settled = true, visible = true, loading = false, shown = 10, floor = 20, askedShown = 10, askedFloor = 40))
        assertTrue(pageEarlier(settled = true, visible = true, loading = false, shown = 12, floor = 40, askedShown = 10, askedFloor = 40))
    }

    @Test fun `a page of only tool or thought rows still moves the window`() {
        // A full window of chat rows, and below it a page from the computer holding no chat row.
        val chatStart = Residency.PAGE + 1L
        val chat = (chatStart until chatStart + Residency.DEFAULT_WINDOW).map { row(it, "user") }
        val page = (1L..Residency.PAGE.toLong()).map { row(it, if (it % 2 == 0L) "tool" else "thought") }
        val stored = (page + chat).sortedByDescending { it.seq }.map { WireJson.encodeToString(it) }
        fun floor(residency: Residency) = newestWindow(stored.asSequence(), residency.limit("a"), Long.MAX_VALUE).minOf { it.seq }
        val open = Residency(setOf("a"))
        assertEquals(chatStart, floor(open))
        assertEquals(1L, floor(open.copy(windows = mapOf("a" to open.widened("a")))))
    }

    @Test fun `a window stops once its rows pass the byte budget, its newest row always kept`() {
        val stored = (10L downTo 1L).map { WireJson.encodeToString(row(it, "user")) }
        val threeRows = stored.take(3).sumOf { it.length }.toLong()
        assertEquals((8L..10L).toList(), newestWindow(stored.asSequence(), 10, threeRows).map { it.seq })
        assertEquals(listOf(10L), newestWindow(stored.asSequence(), 10, 1).map { it.seq })
        assertEquals((6L..10L).toList(), newestWindow(stored.asSequence(), 5, Long.MAX_VALUE).map { it.seq })
    }

    @Test fun `an oversized newest payload stays one short row and keeps its seq`() {
        val filler = "F".repeat(PER_MESSAGE_CHARS + 1)
        val huge = WireJson.encodeToString(row(10, "user").copy(data = buildJsonObject { put("text", filler) }))
        assertTrue(huge.length > PER_MESSAGE_CHARS)
        val kept = newestWindow(sequenceOf(huge), 10, PER_MESSAGE_CHARS.toLong() + 1_000)
        assertEquals(listOf(10L), kept.map { it.seq })
        assertFalse(filler in kept.single().text)
        assertTrue(kept.single().text.length < 200)
        assertTrue(kept.single().isStoredStub)
        assertEquals(STORED_STUB_ID, kept.single().text)
        assertEquals(storedStubText(huge.length.toLong()), kept.single().text)
        assertFalse(kept.single().text.contains(' '))
        assertEquals(huge.length.toLong(), kept.single().data["storedChars"]?.jsonPrimitive?.longOrNull)
    }

    @Test fun `a normal row is not a stub and a tool row shows its output`() {
        val hello = row(1, "user").copy(data = buildJsonObject { put("text", "hello") })
        assertFalse(hello.isStoredStub)
        assertEquals("hello", hello.storedDisplayText())
        val tool = row(2, "tool").copy(data = buildJsonObject { put("output", "tool output") })
        assertEquals("tool output", tool.storedDisplayText())
    }

    @Test fun `a patch-only tool row opens with the patches it made`() {
        val tool = row(2, "tool").copy(data = buildJsonObject {
            put("toolKind", "edit")
            put("title", "Edit two files")
            put("diffs", buildJsonArray {
                add(buildJsonObject { put("path", "src/a.kt"); put("patch", "- old\n+ new") })
                add(buildJsonObject { put("path", "src/b.kt"); put("patch", "+ added") })
            })
        })
        val shown = tool.storedDisplayText()
        assertTrue(shown.contains("src/a.kt"))
        assertTrue(shown.contains("- old\n+ new"))
        assertTrue(shown.contains("src/b.kt"))
        assertTrue(shown.contains("+ added"))
        // Nothing to show of a diff is still not a sheet of nothing: the row's name stands.
        val unnamed = row(3, "tool").copy(data = buildJsonObject { put("title", "Edit") })
        assertEquals("Edit", unnamed.storedDisplayText())
    }

    @Test fun `a stubbed final reply is still a chat row the thread keeps`() {
        val filler = "R".repeat(PER_MESSAGE_CHARS)
        val reply = row(3, "agent").copy(data = buildJsonObject { put("text", filler); put("final", true) })
        val payload = WireJson.encodeToString(reply)
        assertTrue(payload.length > PER_MESSAGE_CHARS)
        assertTrue("\"final\" is after the body, so the tail is what keeps it", payload.takeLast(PAYLOAD_TAIL_CHARS).contains("\"final\""))
        val kept = newestWindow(sequenceOf(payload), 10, PER_MESSAGE_CHARS.toLong() + 1_000).single()
        assertTrue(kept.isStoredStub)
        assertEquals("true", kept.data["final"]?.jsonPrimitive?.content)
        assertTrue("a thread lists its rows by isChat, so a stub reply has to stay one", kept.isChat)
    }

    @Test fun `a stubbed reply keeps its kind when a large member follows its final flag`() {
        val filler = "R".repeat(PER_MESSAGE_CHARS)
        // The host writes `final` after the body, then the quoted reply, which is large too: neither the head nor the
        // tail of the row holds the flag.
        val reply = row(3, "agent").copy(data = buildJsonObject {
            put("text", filler)
            put("final", true)
            putJsonObject("replyTo") {
                put("id", "e-1")
                put("kind", "user")
                put("text", "Q".repeat(4_000))
            }
        })
        val payload = WireJson.encodeToString(reply)
        assertTrue(payload.length > PER_MESSAGE_CHARS)
        assertFalse("the head stops inside the body", "\"final\"" in payload.take(PAYLOAD_HEAD_CHARS))
        assertFalse("the tail holds the end of the quote", "\"final\"" in payload.takeLast(PAYLOAD_TAIL_CHARS))
        val kept = newestWindow(sequenceOf(payload), 10, PER_MESSAGE_CHARS.toLong() + 2_000).single()
        assertTrue(kept.isStoredStub)
        assertEquals("true", kept.data["final"]?.jsonPrimitive?.content)
        assertTrue("the reply stays in the thread whatever the row writes after its flag", kept.isChat)

        // The same for a tool row's bookkeeping flag, written before the files it produced.
        val tool = row(5, "tool").copy(turn = 1, data = buildJsonObject {
            put("toolKind", "execute")
            put("output", filler)
            put("bookkeeping", false)
            put("files", buildJsonArray { add(buildJsonObject { put("name", "F".repeat(4_000)) }) })
        })
        val toolPayload = WireJson.encodeToString(tool)
        assertFalse("\"bookkeeping\"" in toolPayload.take(PAYLOAD_HEAD_CHARS))
        assertFalse("\"bookkeeping\"" in toolPayload.takeLast(PAYLOAD_TAIL_CHARS))
        val keptTool = newestWindow(sequenceOf(toolPayload), 10, PER_MESSAGE_CHARS.toLong() + 2_000).single()
        assertTrue(keptTool.isStoredStub)
        assertEquals("false", keptTool.data["bookkeeping"]?.jsonPrimitive?.content)
        assertTrue("the tool stays a working step", keptTool.isWorkingStep)
    }

    @Test fun `a stubbed tool row is still one of the working steps`() {
        val filler = "T".repeat(PER_MESSAGE_CHARS)
        val tool = row(4, "tool").copy(turn = 1, data = buildJsonObject {
            put("toolKind", "execute")
            put("title", "npm test")
            put("output", filler)
            put("bookkeeping", false)
        })
        val payload = WireJson.encodeToString(tool)
        assertTrue(payload.length > PER_MESSAGE_CHARS)
        val kept = newestWindow(sequenceOf(payload), 10, PER_MESSAGE_CHARS.toLong() + 1_000).single()
        assertTrue(kept.isStoredStub)
        assertTrue("a tool is a step only when bookkeeping is explicitly false", kept.isWorkingStep)
        assertEquals(listOf(kept.seq), workingSteps(listOf(kept)).map { it.seq })
        // The bot's own bookkeeping stays out of the steps, stub or not.
        val bookkeeping = kept.copy(data = JsonObject(kept.data + ("bookkeeping" to JsonPrimitive(true))))
        assertFalse(bookkeeping.isWorkingStep)
    }

    @Test fun `an oversized newest row stays a stub and an older row still loads`() {
        val filler = "F".repeat(PER_MESSAGE_CHARS + 1)
        val huge = WireJson.encodeToString(row(10, "user").copy(data = buildJsonObject { put("text", filler) }))
        val older = WireJson.encodeToString(row(9, "user"))
        val kept = newestWindow(sequenceOf(huge, older), 10, PER_MESSAGE_CHARS.toLong())
        assertEquals(listOf(9L, 10L), kept.map { it.seq })
        assertFalse(filler in kept.last().text)
        assertEquals("user", kept.first().kind)
    }

    @Test fun `a normal row that does not fit is left out once the window has a row`() {
        val stored = (10L downTo 1L).map { WireJson.encodeToString(row(it, "user")) }
        val one = stored.first().length.toLong()
        assertEquals(listOf(10L), newestWindow(stored.asSequence(), 10, one).map { it.seq })
    }

    @Test fun `a huge tool row does not count toward the shown chat rows`() {
        val filler = "T".repeat(PER_MESSAGE_CHARS + 8)
        val tool = WireJson.encodeToString(row(5, "tool").copy(data = buildJsonObject { put("text", filler) }))
        val users = (4L downTo 1L).map { WireJson.encodeToString(row(it, "user")) }
        val window = newestWindow(sequenceOf(tool) + users.asSequence(), 2, Long.MAX_VALUE)
        assertEquals(listOf(3L, 4L, 5L), window.map { it.seq })
        assertEquals(2, window.count { it.isChat })
        assertEquals("tool", window.last().kind)
        assertFalse(filler in window.last().text)
    }

    @Test fun `a head without a seq is dropped instead of kept as zero`() {
        val broken = "x".repeat(PER_MESSAGE_CHARS + 1)
        val older = WireJson.encodeToString(row(4, "user"))
        assertEquals(listOf(4L), newestWindow(sequenceOf(broken, older), 10, Long.MAX_VALUE).map { it.seq })
    }

    @Test fun `an oversized row keeps its times so it stays where it was written`() {
        val filler = "F".repeat(PER_MESSAGE_CHARS + 1)
        val huge = WireJson.encodeToString(row(10, "user").copy(createdAt = 1_700_000_000_000, updatedAt = 1_700_000_001_000,
            data = buildJsonObject { put("text", filler) }))
        val kept = newestWindow(sequenceOf(huge), 10, PER_MESSAGE_CHARS.toLong() + 1_000).single()
        assertTrue(kept.isStoredStub)
        assertEquals(1_700_000_000_000, kept.createdAt)
        assertEquals(1_700_000_001_000, kept.updatedAt)
    }

    @Test fun `a normal row keeps the times it was stored with`() {
        val kept = newestWindow(sequenceOf(WireJson.encodeToString(row(3, "user").copy(createdAt = 42))), 10, Long.MAX_VALUE).single()
        assertFalse(kept.isStoredStub)
        assertEquals(42, kept.createdAt)
    }

    @Test fun `a stubbed approval is marked so its whole row is read before it is answered`() {
        val filler = "D".repeat(PER_MESSAGE_CHARS + 1)
        val approval = WireJson.encodeToString(row(2, "permission").copy(data = buildJsonObject { put("title", "Edit"); put("detail", filler) }))
        val kept = newestWindow(sequenceOf(approval), 10, PER_MESSAGE_CHARS.toLong() + 1_000).single()
        assertTrue(kept.isStoredStub)
        assertTrue(kept.isApprovalStub)
        assertFalse(row(3, "user").isApprovalStub)
    }

    @Test fun `a malformed short approval becomes a readable stub instead of aborting the snapshot`() {
        val approval = row(2, "permission").copy(data = buildJsonObject { put("title", "Edit"); put("status", "pending") })
        val broken = WireJson.encodeToString(approval).dropLast(1)
        val kept = newestWindow(sequenceOf(broken), 10, Long.MAX_VALUE).single()
        assertEquals(2L, kept.seq)
        assertTrue(kept.isStoredStub)
        assertTrue(kept.isApprovalStub)
    }

    @Test fun `a host approval keeps its separately stored thread when the marker follows a large body`() {
        val approval = row(2, "permission").copy(botId = "b", data = buildJsonObject {
            put("detail", """A detail that mentions "thread":{"id":"not-the-thread"} and then keeps going: ${"D".repeat(40_000)}""")
            put("title", "Edit")
            put("status", "pending")
            put("author", "b")
            putJsonObject("thread") { put("id", "thread-1"); put("title", "A card from its thread") }
            put("task", "task-1")
        })
        val broken = WireJson.encodeToString(approval).dropLast(1)
        assertTrue(broken.length > PER_MESSAGE_CHARS)
        // A marker in the tail is not routing evidence: without metadata the stub stays in the owner chat.
        val unstored = newestWindow(sequenceOf(broken), 10, Long.MAX_VALUE).single()
        assertTrue(unstored.isStoredStub)
        assertEquals(null, unstored.data["thread"])
        // Routing stored beside the payload is, wherever in the row the marker sits.
        val kept = windowRows(sequenceOf(StoredRow(
            broken.take(PAYLOAD_HEAD_CHARS),
            broken.length.toLong(),
            broken.takeLast(PAYLOAD_TAIL_CHARS),
            "thread-1",
        )), 10, Long.MAX_VALUE).single().entry
        assertTrue(kept.isStoredStub)
        assertTrue(kept.isApprovalStub)
        val card = threadChatItems(emptyList(), listOf(kept.withApprovalReadFailure()), "thread-1", emptyList(), live = false)
            .filterIsInstance<ChatItem.Message>().single().entry
        assertEquals("e-2", card.id)
        assertTrue(card.approvalReadFailed)
        assertEquals("thread-1", ((card.data["thread"] as kotlinx.serialization.json.JsonObject)["id"] as kotlinx.serialization.json.JsonPrimitive).content)
    }

    @Test fun `only a nonblank string thread id is routing metadata`() {
        fun routed(id: JsonElement?) = row(2, "permission").copy(data = buildJsonObject {
            if (id == null) putJsonObject("thread") { put("title", "No id") }
            else putJsonObject("thread") { put("id", id); put("title", "A thread") }
        }).routingThreadId()

        assertEquals("thread-1", routed(JsonPrimitive("thread-1")))
        assertEquals(null, routed(JsonPrimitive("")))
        assertEquals(null, routed(JsonPrimitive("   ")))
        assertEquals(null, routed(JsonNull))
        assertEquals(null, routed(buildJsonObject { put("nested", "thread-2") }))
        assertEquals(null, routed(buildJsonArray { add(JsonPrimitive("thread-2")) }))
        assertEquals(null, routed(JsonPrimitive(7)))
        assertEquals(null, routed(JsonPrimitive(true)))
        assertEquals(null, routed(null))
        assertEquals(null, row(2, "permission").routingThreadId())
    }

    @Test fun `an unreadable approval keeps the route stored beside it and shows there as a warning`() {
        val approval = row(2, "permission").copy(botId = "b", data = buildJsonObject {
            putJsonObject("thread") { put("id", "thread-1") }
            put("title", "Edit")
            put("status", "pending")
        })
        val broken = WireJson.encodeToString(approval).dropLast(1)
        // The route is the column the schema stored with the row, so a payload that no longer decodes still has one.
        val kept = windowRows(sequenceOf(StoredRow(broken, broken.length.toLong(), null, "thread-1")),
            10, Long.MAX_VALUE).single().entry
        assertTrue(kept.isStoredStub)
        assertTrue(kept.isApprovalStub)
        val thread = kept.data["thread"] as kotlinx.serialization.json.JsonObject
        assertEquals("thread-1", (thread["id"] as kotlinx.serialization.json.JsonPrimitive).content)
        val warning = kept.withApprovalReadFailure()
        val card = threadChatItems(emptyList(), listOf(warning), "thread-1", emptyList(), live = false)
            .filterIsInstance<ChatItem.Message>().single().entry
        assertEquals("e-2", card.id)
        assertTrue(card.approvalReadFailed)
        assertTrue(card.isApprovalStub)
        assertEquals("thread-1", ((card.data["thread"] as kotlinx.serialization.json.JsonObject)["id"] as kotlinx.serialization.json.JsonPrimitive).content)
    }

    @Test fun `a short row is never routed by a thread marker in its text`() {
        val approval = row(2, "permission").copy(data = buildJsonObject {
            putJsonObject("thread") { put("id", "thread-in-text") }
            put("status", "pending")
        })
        val payload = WireJson.encodeToString(approval)
        // The route is the column stored beside the row; a row with none has no route, whatever its text says.
        val kept = windowRows(sequenceOf(StoredRow(payload, payload.length.toLong())), 10, Long.MAX_VALUE).single().entry
        assertEquals(null, kept.data["thread"])
    }

    @Test fun `an approval whose payload disagrees with its stored route keeps no answers`() {
        val approval = row(2, "permission").copy(data = buildJsonObject {
            putJsonObject("thread") { put("id", "thread-b") }
            put("status", "pending")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
        })
        val payload = WireJson.encodeToString(approval)
        val kept = windowRows(sequenceOf(StoredRow(payload, payload.length.toLong(), null, "thread-a")),
            10, Long.MAX_VALUE).single().entry
        assertTrue("an unproven route keeps the card a stub", kept.isStoredStub)
        assertEquals("thread-a", ((kept.data["thread"] as JsonObject)["id"] as JsonPrimitive).content)
        assertTrue("no answers under a route the row does not name", kept.data["options"] == null)
        // The same row, routed the way it names itself, reads whole and keeps its answers.
        val agree = windowRows(sequenceOf(StoredRow(payload, payload.length.toLong(), null, "thread-b")),
            10, Long.MAX_VALUE).single().entry
        assertFalse(agree.isStoredStub)
        assertEquals("thread-b", ((agree.data["thread"] as JsonObject)["id"] as JsonPrimitive).content)
        assertEquals("pending", agree.status)
    }

    @Test fun `a preview that does not fit is dropped whole and a later one still joins`() {
        val rows = mapOf(
            "priority" to listOf(KeptRow(row(1, "user"), 1), KeptRow(row(2, "user"), 1)),
            "later" to listOf(KeptRow(row(3, "user"), 1)),
        )
        assertEquals(listOf(1L, 2L), previewsWithin(listOf("priority", "later"), 2) { rows.getValue(it) }.map { it.seq })
        assertTrue(previewsWithin(emptyList(), 10) { rows.getValue(it) }.isEmpty())
    }

    @Test fun `an unopened chat whose preview does not fit is left out and a smaller one still joins`() {
        val rows = mapOf(
            "tooBig" to listOf(KeptRow(row(1, "user"), 2), KeptRow(row(2, "user"), 2)),
            "small" to listOf(KeptRow(row(3, "user"), 2)),
        )
        assertTrue(previewsWithin(listOf("tooBig"), 3) { rows.getValue(it) }.isEmpty())
        assertEquals(listOf(3L), previewsWithin(listOf("tooBig", "small"), 3) { rows.getValue(it) }.map { it.seq })
    }

    @Test fun `a preview the budget refuses is left out of the kept ones`() {
        val rows = mapOf(
            "first" to listOf(KeptRow(row(1, "user"), 3)),
            "second" to listOf(KeptRow(row(2, "user"), 3)),
        )
        val asked = mutableListOf<String>()
        val kept = previewsWithin(listOf("first", "second"), 3) { asked += it; rows.getValue(it) }
        assertEquals("each chat's rows are read as its turn comes", listOf("first", "second"), asked)
        assertEquals(listOf(1L), kept.map { it.seq })
    }

    @Test fun `the chats the reader opened come first in the preview order`() {
        assertEquals(listOf("c", "a", "b"), previewOrder(listOf("c", "gone"), listOf("a", "b", "c")))
        assertEquals(listOf("a", "c"), previewOrder(listOf("a", "a", "c"), listOf("a", "c", "a")))
        assertEquals(listOf("a"), previewOrder(emptyList(), listOf("a")))
    }

    @Test fun `a page that failed is asked for again once the row shows anew, never in a loop`() {
        val paging = EarlierPaging()
        assertTrue(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
        assertFalse(paging.ask(settled = true, visible = true, loading = true, shown = 10, floor = 40))
        paging.failed()
        // The load ended with the row still up: no second request right away.
        assertFalse(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
        assertFalse(paging.ask(settled = true, visible = false, loading = false, shown = 10, floor = 40))
        assertTrue(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
        // A page that loaded but moved nothing isn't asked for again when the row comes back.
        assertFalse(paging.ask(settled = true, visible = false, loading = false, shown = 10, floor = 40))
        assertFalse(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
    }

    @Test fun `a failed page is asked for again after the list dropped its row unseen`() {
        val paging = EarlierPaging()
        assertTrue(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
        paging.failed()
        // Scrolled far away: the row leaves composition without ever asking while hidden.
        paging.left()
        assertTrue(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
        // Leaving after a page that loaded asks nothing more.
        paging.left()
        assertFalse(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
    }

    @Test fun `opening an item's card pages back until the chat holds it, never past the end or five pages`() = runBlocking {
        var pages = 0
        pageUntil(found = { pages == 2 }, more = { true }) { pages++ }
        assertEquals("found in the second page", 2, pages)
        pages = 0
        pageUntil(found = { false }, more = { pages < 1 }) { pages++ }
        assertEquals("the chat's history ended", 1, pages)
        pages = 0
        pageUntil(found = { false }, more = { true }) { pages++ }
        assertEquals("never found", 5, pages)
    }

    @Test fun `an oversized pending approval stub keeps its status and options`() {
        val filler = "F".repeat(PER_MESSAGE_CHARS)
        val approval = row(2, "permission").copy(data = buildJsonObject {
            put("status", "pending")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
                add(buildJsonObject { put("optionId", "deny"); put("name", "Deny"); put("kind", "reject_once") })
            })
            put("detail", filler)
        })
        val payload = WireJson.encodeToString(approval)
        assertTrue(payload.length > PER_MESSAGE_CHARS)
        val kept = newestWindow(sequenceOf(payload), 10, PER_MESSAGE_CHARS.toLong() + 1_000).single()
        assertTrue(kept.isStoredStub)
        assertEquals(storedStubText(payload.length.toLong()), kept.text)
        assertFalse(filler in kept.text)
        assertEquals("pending", kept.status)
        val options = kept.data["options"] as JsonArray
        assertEquals(listOf("allow", "deny"), options.map { (it as JsonObject).text("optionId") })

        // A queued message carries its action fields after the body. The stub must still be queued.
        val queuedPayload = WireJson.encodeToString(row(3, "user").copy(data = buildJsonObject {
            put("text", filler)
            put("clientNonce", "nonce-1")
            put("status", "queued")
        }))
        val queued = newestWindow(sequenceOf(queuedPayload), 10, PER_MESSAGE_CHARS.toLong() + 1_000).single()
        assertEquals(storedStubText(queuedPayload.length.toLong()), queued.text)
        assertFalse(filler in queued.text)
        assertEquals("queued", queued.status)
        assertEquals("nonce-1", queued.clientNonce)
        assertTrue(queued.isQueued)
    }

    @Test fun `a stubbed pending approval keeps the title and tool kind that name it`() {
        val filler = "F".repeat(PER_MESSAGE_CHARS)
        val approval = row(2, "permission").copy(data = buildJsonObject {
            put("status", "pending")
            put("toolKind", "execute")
            put("title", "npm test")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
            put("detail", filler)
        })
        val payload = WireJson.encodeToString(approval)
        assertTrue(payload.length > PER_MESSAGE_CHARS)
        val kept = newestWindow(sequenceOf(payload), 10, PER_MESSAGE_CHARS.toLong() + 1_000).single()
        assertEquals("npm test", kept.data.text("title"))
        assertEquals("execute", kept.data.text("toolKind"))
        assertEquals(null, kept.data["detail"])
        assertEquals(null, kept.data["command"])
        assertEquals("Wants to run a command", approvalHeadline(kept.data.text("toolKind")))
        assertFalse(canAnswerStub(kept))
        val unnamed = kept.copy(data = JsonObject(kept.data.filterKeys { it != "options" }))
        assertFalse(canAnswerStub(unnamed))
        assertTrue(kept.withApprovalReadFailure().approvalReadFailed)
        assertFalse(canAnswerStub(kept.withApprovalReadFailure()))

        val plain = row(4, "permission").copy(data = buildJsonObject {
            put("status", "pending")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
            put("detail", filler)
        })
        val generic = newestWindow(sequenceOf(WireJson.encodeToString(plain)), 10, Long.MAX_VALUE).single()
        assertEquals(null, generic.data.text("title"))
        assertEquals(null, generic.data.text("toolKind"))
        assertEquals("Wants to use a tool", approvalHeadline(generic.data.text("toolKind")))

        // The name sits after the body, so only the tail can see it. It is not copied from there.
        val late = row(5, "permission").copy(data = buildJsonObject {
            put("detail", filler)
            put("title", "npm test")
            put("toolKind", "execute")
        })
        val latePayload = WireJson.encodeToString(late)
        assertTrue(latePayload.takeLast(PAYLOAD_TAIL_CHARS).contains("\"title\""))
        assertFalse(latePayload.take(PAYLOAD_HEAD_CHARS).contains("\"title\""))
        val fromTail = windowRows(sequenceOf(StoredRow(
            latePayload.take(PAYLOAD_HEAD_CHARS), latePayload.length.toLong(), latePayload.takeLast(PAYLOAD_TAIL_CHARS),
        )), 10, Long.MAX_VALUE).single().entry
        assertEquals(null, fromTail.data.text("title"))
        assertEquals(null, fromTail.data.text("toolKind"))

        val older = WireJson.encodeToString(row(1, "user"))
        val named = windowRows(sequenceOf(
            StoredRow(payload.take(PAYLOAD_HEAD_CHARS), payload.length.toLong(), payload.takeLast(PAYLOAD_TAIL_CHARS)),
            StoredRow(older, older.length.toLong()),
        ), 10, kept.storedStubSentence()!!.length + older.length.toLong())
        assertEquals(listOf(2L), named.map { it.entry.seq })
        assertEquals(
            kept.storedStubSentence()!!.length + "pending".length + "execute".length + "npm test".length +
                "allow".length + "Allow".length + "allow_once".length,
            named.single().cost,
        )
        assertEquals(named.sumOf { it.cost }, named.sumOf { retainedStubChars(it.entry) })
    }

    @Test fun `a stub's counted cost is the sentence plus every key it kept`() {
        val filler = "F".repeat(PER_MESSAGE_CHARS)
        val threadId = "thread-retained"
        val approval = row(7, "permission").copy(data = buildJsonObject {
            put("status", "pending")
            put("selected", "allow")
            put("clientNonce", "nonce-retained")
            put("removed", false)
            put("toolKind", "execute")
            put("title", "npm test")
            put("options", buildJsonArray {
                add(buildJsonObject { put("optionId", "allow"); put("name", "Allow"); put("kind", "allow_once") })
            })
            put("detail", filler)
            putJsonObject("thread") { put("id", "not-the-column") }
        })
        val payload = WireJson.encodeToString(approval)
        assertTrue(payload.length > PER_MESSAGE_CHARS)
        val kept = windowRows(sequenceOf(StoredRow(
            payload.take(PAYLOAD_HEAD_CHARS), payload.length.toLong(), payload.takeLast(PAYLOAD_TAIL_CHARS), threadId,
        )), 10, Long.MAX_VALUE).single()
        val entry = kept.entry
        assertEquals("pending", entry.data.text("status"))
        assertEquals("allow", entry.data.text("selected"))
        assertEquals("nonce-retained", entry.clientNonce)
        assertEquals(false, entry.isRemoved)
        assertEquals("execute", entry.data.text("toolKind"))
        assertEquals("npm test", entry.data.text("title"))
        assertEquals(listOf("allow"), (entry.data["options"] as JsonArray).map { (it as JsonObject).text("optionId") })
        assertEquals(threadId, entry.routingThreadId())
        assertEquals("the body is not kept on the stub", null, entry.data["detail"])
        val expected = entry.storedStubSentence()!!.length +
            "pending".length + "allow".length + "nonce-retained".length + "false".length +
            "execute".length + "npm test".length +
            "allow".length + "Allow".length + "allow_once".length +
            threadId.length
        assertEquals(expected, kept.cost)
        assertEquals(expected, retainedStubChars(entry))
        val withoutNonce = entry.copy(data = JsonObject(entry.data.filterKeys { it != "clientNonce" }))
        assertEquals(expected - "nonce-retained".length, retainedStubChars(withoutNonce))
        val withBody = entry.copy(data = JsonObject(entry.data + ("detail" to JsonPrimitive(filler))))
        assertEquals("a key the stub does not keep is not counted", expected, retainedStubChars(withBody))
    }

    @Test fun `stub copy is an id and every catalog has the sentence and the sheet title`() {
        val keys = listOf("Stored message", STORED_STUB_CAPTION)
        val dir = java.io.File("../../ios/Kit/Sources/CodyncKit/Resources/Languages")
        for (tag in listOf("zh-Hans", "zh-Hant", "ja", "ko", "fr", "de", "es", "pt-BR", "ru", "it")) {
            val table = Json.parseToJsonElement(java.io.File(dir, "$tag.json").readText()).jsonObject
            for (key in keys) assertTrue("$tag is missing $key", table.containsKey(key))
        }
    }

    private fun row(seq: Long, kind: String) = Entry("e-$seq", seq, "a", rev = seq, kind = kind)
}
