package com.codync.android.core

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class BdotsModelsTest {

    @Test
    fun `bot decodes avatarLook actionRules and proactive settings from host JSON`() {
        val raw = WireJson.parseToJsonElement(
            """
            {
              "id": "bot-1",
              "name": "Scout",
              "rev": 3,
              "avatarColor": "cyan",
              "avatarShape": "blob",
              "avatarLook": "visor",
              "actionRules": {
                "files": "allow",
                "shell": "ask",
                "routines": "block"
              },
              "proactive": {
                "enabled": true,
                "everyHours": 2,
                "quietStart": 23,
                "quietEnd": 7,
                "maxPerDay": 5,
                "appsInBackground": false
              }
            }
            """.trimIndent()
        ).jsonObject

        val bot = HostClient.decode(raw)
        assertEquals("visor", bot.avatarLook)
        assertEquals("allow", bot.actionRules["files"])
        assertEquals("ask", bot.actionRules["shell"])
        assertEquals("block", bot.actionRules["routines"])
        assertEquals(true, bot.proactive?.enabled)
        assertEquals(2, bot.proactive?.everyHours)
        assertEquals(23, bot.proactive?.quietStart)
        assertEquals(7, bot.proactive?.quietEnd)
        assertEquals(false, bot.proactive?.appsInBackground)

        val draft = BotDraft.from(bot)
        assertEquals("visor", draft.avatarLook)
        assertEquals("allow", draft.actionRules["files"])
        assertEquals(true, draft.proactive?.enabled)

        val body = draft.body()
        assertEquals("visor", body["avatarLook"]?.jsonPrimitive?.content)
        assertTrue("proactive" in body)
    }

    @Test
    fun `unset proactive stays out of BotDraft body payload`() {
        val draft = BotDraft(id = "bot-2", name = "DefaultBot")
        val body = draft.body()
        assertNull(body["proactive"])
    }

    @Test
    fun `BotApp and OpenItem serialize and deserialize cleanly`() {
        val appJson = WireJson.parseToJsonElement(
            """
            {
              "slug": "budget-tracker",
              "botId": "bot-1",
              "title": "Budget Tracker",
              "description": "Live spending overview",
              "version": 4,
              "url": "/apps/bot-1/budget-tracker/tok123/",
              "links": [
                {
                  "token": "tok123",
                  "botId": "bot-1",
                  "slug": "budget-tracker",
                  "label": "Team",
                  "mode": "edit",
                  "opens": 7
                }
              ],
              "collections": [
                {
                  "collection": "expenses",
                  "count": 12,
                  "updatedAt": 1700000000
                }
              ]
            }
            """.trimIndent()
        )
        val app = WireJson.decodeFromJsonElement<BotApp>(appJson)
        assertEquals("budget-tracker", app.slug)
        assertEquals(4, app.version)
        assertEquals(1, app.links.size)
        assertEquals("edit", app.links.first().mode)
        assertEquals(12, app.collections.first().count)

        val itemJson = WireJson.encodeToJsonElement(
            OpenItem(
                id = "item-1",
                botId = "bot-1",
                title = "Follow up on PR #42",
                note = "Check CI status",
                status = "open",
            )
        ).jsonObject
        val decodedItem = WireJson.decodeFromJsonElement<OpenItem>(JsonObject(itemJson))
        assertEquals("item-1", decodedItem.id)
        assertEquals("Follow up on PR #42", decodedItem.title)
        assertEquals("Check CI status", decodedItem.note)
    }

    @Test
    fun `lockScreenSummary enforces privacy by default and classifies freshness`() {
        val now = 1_700_000_000_000L
        val bots = listOf(
            WidgetBot("b1", "Alpha", "working", lastAt = now - 60_000L),
            WidgetBot("b2", "Beta", "needsInput", lastAt = now - 30_000L),
            WidgetBot("b3", "Gamma", "idle", lastAt = now - 120_000L),
        )
        val freshFeed = WidgetFeed(
            scope = "user-1",
            computerId = "comp-1",
            computerName = "Edwin-PC",
            bots = bots,
            botsAt = now - 10 * 60_000L,
        )

        val defaultSummary = lockScreenSummary(freshFeed, nowMillis = now)
        assertEquals(WidgetFreshness.Fresh, defaultSummary.freshness)
        assertEquals(1, defaultSummary.workingCount)
        assertEquals(1, defaultSummary.attentionCount)
        assertEquals(10L, defaultSummary.ageMinutes)
        assertNull(defaultSummary.computerName)
        assertNull(defaultSummary.leadBotName)

        val detailedSummary = lockScreenSummary(freshFeed, nowMillis = now, includeDetails = true)
        assertEquals("Edwin-PC", detailedSummary.computerName)
        assertEquals("Beta", detailedSummary.leadBotName)

        val staleFeed = freshFeed.copy(botsAt = now - 90 * 60_000L)
        assertEquals(WidgetFreshness.Stale, lockScreenSummary(staleFeed, nowMillis = now).freshness)

        val unpairedFeed = freshFeed.copy(computerId = null)
        assertEquals(WidgetFreshness.NoComputer, lockScreenSummary(unpairedFeed, nowMillis = now).freshness)
    }

    @Test
    fun `sharedWorkspacePeers matches Windows and Unix paths and ignores managed or idle bots`() {
        val b1 = Bot(id = "1", name = "Coder", cwd = "C:\\Users\\Edwin\\Repo\\", status = "working")
        val b2 = Bot(id = "2", name = "Reviewer", cwd = "c:/users/edwin/repo", status = "needsInput")
        val b3 = Bot(id = "3", name = "IdlePeer", cwd = "c:/users/edwin/repo", status = "idle")
        val b4 = Bot(id = "4", name = "ManagedPeer", cwd = "c:/users/edwin/repo", managedWorkspace = true, status = "working")

        val peers = sharedWorkspacePeers(b1, listOf(b1, b2, b3, b4))
        assertEquals(listOf("2"), peers.map { it.id })
    }

    @Test
    fun `collectPendingDecisions collectDeliverables WORKER_ROLE_TEMPLATES and polishVoiceDictation work`() {
        val entries = listOf(
            Entry(
                id = "p1",
                seq = 1,
                botId = "b1",
                rev = 1,
                kind = "permission",
                turn = 1,
                createdAt = 200L,
                updatedAt = 200L,
                data = WireJson.parseToJsonElement("""{"title":"Run shell?","text":"cargo test","status":"pending"}""").jsonObject,
            ),
            Entry(
                id = "p2",
                seq = 2,
                botId = "b1",
                rev = 2,
                kind = "permission",
                turn = 1,
                createdAt = 100L,
                updatedAt = 100L,
                data = WireJson.parseToJsonElement("""{"title":"Done","status":"approved"}""").jsonObject,
            ),
            Entry(
                id = "a1",
                seq = 3,
                botId = "b1",
                rev = 3,
                kind = "agent",
                turn = 1,
                createdAt = 300L,
                updatedAt = 300L,
                data = WireJson.parseToJsonElement(
                    """{"text":"Here is the artifact","files":[{"id":"f1","name":"summary.md","size":4096,"sha256":"1234567890abcdef"}]}"""
                ).jsonObject,
            ),
        )
        val decisions = collectPendingDecisions(entries)
        assertEquals(1, decisions.size)
        assertEquals("p1", decisions.first().entryId)
        assertEquals("permission", decisions.first().kind)

        val deliverables = collectDeliverables(entries)
        assertEquals(1, deliverables.size)
        assertEquals("summary.md", deliverables.first().name)

        assertEquals(5, WORKER_ROLE_TEMPLATES.size)
        assertEquals(
            "Revisa el archivo, por favor. Gracias!",
            polishVoiceDictation("  um, revisa el archivo ,por favor.   uh gracias! "),
        )

        val connectorJson = WireJson.parseToJsonElement(
            """{"id":"github","name":"GitHub","kind":"remote","lastFailure":{"kind":"auth","message":"Token expired","at":1700000000}}"""
        )
        val connector = WireJson.decodeFromJsonElement<InstalledConnector>(connectorJson)
        assertEquals("auth", connector.lastFailure?.kind)
        assertEquals("Token expired", connector.lastFailure?.message)
    }

    private fun entry(id: String, seq: Long, kind: String, data: String) =
        Entry(id, seq, "b1", seq, kind, 1, WireJson.parseToJsonElement(data).jsonObject, createdAt = seq * 100, updatedAt = seq * 100)

    @Test
    fun `pending decisions use the entries the host really writes`() {
        val entries = listOf(
            entry("p", 1, "permission", """{"title":"Run","status":"pending","options":[
                {"optionId":"yes","name":"Allow","kind":"allow_once"},{"optionId":"no","name":"Reject","kind":"reject_once"}]}"""),
            entry("h", 2, "notice", """{"text":"Human takeover requested: log in","takeoverRequest":{"reason":"log in","status":"waiting"}}"""),
            entry("hd", 3, "notice", """{"text":"x","takeoverRequest":{"reason":"done","status":"resolved"}}"""),
            entry("hx", 3, "notice", """{"text":"x","takeoverRequest":{"reason":"late","status":"expired"}}"""),
            entry("e", 4, "notice", """{"text":"The agent stopped","style":"error"}"""),
        )
        val byId = collectPendingDecisions(entries).associateBy { it.entryId }
        assertEquals(setOf("p", "h", "e"), byId.keys)
        assertEquals("yes", byId.getValue("p").allowOptionId)
        assertEquals("no", byId.getValue("p").denyOptionId)
        assertEquals("log in", byId.getValue("h").detail)
        // Once the bot or the person writes again, the error is behind them.
        val moved = entries + entry("a", 5, "agent", """{"text":"Back on it"}""")
        assertEquals(setOf("p", "h"), collectPendingDecisions(moved).map { it.entryId }.toSet())
    }
}
