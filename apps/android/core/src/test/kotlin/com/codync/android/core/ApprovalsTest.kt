package com.codync.android.core

import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class ApprovalsTest {
    private val codex = AgentModels(emptyList(), modes = listOf(
        AgentMode("auto", "Auto review", "Only ask for actions detected as potentially unsafe"), AgentMode("read-only", "Read-only")))

    private fun labels(draft: BotDraft, catalog: AgentModels?) = ApprovalChoice.choices(draft, catalog).first.map { it.label }

    @Test fun newBotsStartInTheAgentsAutoModeAndAsk() {
        val draft = BotDraft(name = "New")
        assertEquals("ask" to "auto", draft.permission to draft.mode)
        assertEquals("auto", draft.body().getValue("mode").jsonPrimitive.content)
    }

    @Test fun approvalsListTheAgentsModesThenApproveAutomatically() {
        var draft = BotDraft(name = "Bot", backend = "codex")
        val (choices, selected) = ApprovalChoice.choices(draft, codex)
        assertEquals(listOf("Auto review", "Read-only", "Approve automatically"), choices.map { it.label })
        assertEquals("Auto review" to "Only ask for actions detected as potentially unsafe", selected.label to selected.note)
        draft = draft.withApproval(choices[1].id)
        assertEquals("ask" to "read-only", draft.permission to draft.mode)
        draft = draft.withApproval(ApprovalChoice.APPROVE)
        assertEquals("auto" to null, draft.permission to draft.mode)
        // Approve automatically clears the mode on the host too.
        assertEquals(JsonNull, draft.copy(id = "b").body()["mode"])
        assertEquals("Approve automatically", ApprovalChoice.choices(draft, codex).second.label)
    }

    @Test fun aBotNeverShowsAsMoreCarefulThanItIs() {
        // Without a mode it works in the agent's default, and asks.
        val unset = BotDraft(name = "Bot", mode = null)
        assertEquals(listOf("Auto review", "Read-only", "Ask me", "Approve automatically"), labels(unset, codex))
        assertEquals("Ask me", ApprovalChoice.choices(unset, codex).second.label)
        // A mode the list lacks (or that hasn't loaded) shows as itself.
        val bypass = BotDraft(name = "Bot", mode = "bypassPermissions")
        assertEquals(listOf("Auto", "Ask me", "bypassPermissions", "Approve automatically"), labels(bypass, null))
        assertEquals("bypassPermissions", ApprovalChoice.choices(bypass, null).second.label)
    }

    @Test fun anAgentWithoutAutoAsksAndOneStillLoadingOffersAuto() {
        val plain = AgentModels(emptyList(), modes = emptyList())
        assertEquals(listOf("Ask me", "Approve automatically"), labels(BotDraft(name = "Bot"), plain))
        assertEquals("Ask me", ApprovalChoice.choices(BotDraft(name = "Bot"), plain).second.label)
        assertEquals(listOf("Auto", "Ask me", "Approve automatically"), labels(BotDraft(name = "Bot"), null))
        // Another agent's ids mean nothing: a picked mode becomes its Auto mode.
        assertEquals("auto", BotDraft(name = "Bot", mode = "plan").withAgent("codex").mode)
        assertEquals(null, BotDraft(name = "Bot", mode = null).withAgent("codex").mode)
    }
}
