package com.codync.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SkillProposalTest {
    private val proposal = SkillProposal("p1", "b", "Ship", "When publishing", "Use this when: publishing\n\n1. Test\n2. Tag\n3. Push\n4. Announce\n",
        listOf("edit", "execute:cargo", "read"))

    @Test fun `the host's inbox decodes and extra fields are ignored`() {
        val json = """{"proposals":[{"id":"p1","botId":"b","name":"Ship","description":"d","instructions":"1. a","fingerprint":"ff","tools":["read"],"createdAt":5}]}"""
        val list = SkillProposal.decodeAll(Json.parseToJsonElement(json))
        assertEquals(listOf("p1"), list.map { it.id })
        assertEquals(emptyList<SkillProposal>(), SkillProposal.decodeAll(Json.parseToJsonElement("""{"proposals":[]}""")))
    }

    @Test fun `the card shows the first steps and the tools in words`() {
        assertEquals(listOf("1. Test", "2. Tag", "3. Push"), proposal.stepsPreview())
        assertEquals("edit, cargo, read", proposal.toolsText())
        assertEquals("commands", proposal.copy(tools = listOf("execute:")).toolsText())
    }

    @Test fun `a proposal needs all three parts to be approved`() {
        assertTrue(proposal.approvable)
        assertFalse(proposal.copy(name = " ").approvable)
        assertFalse(proposal.copy(description = "").approvable)
    }

    @Test fun `the inbox title counts what waits`() {
        assertEquals("Proposed skills", SkillProposal.inboxTitle(0))
        assertEquals("Proposed skills (3)", SkillProposal.inboxTitle(3))
    }
}
