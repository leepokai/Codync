package com.codync.android.core

import org.junit.Assert.*
import kotlinx.serialization.json.*
import org.junit.Test

class AgentSettingsTest {
    @Test fun `agent default folds into one choice while an unknown saved model survives`() {
        val catalog = WireJson.decodeFromString<AgentModels>("""{"models":[{"id":"default-id","name":"Default (recommended)","description":"Agent choice"},{"id":"on-host","name":"Host model"}],"currentModelId":"on-host","future":true}""")
        val choices = catalog.options("removed-model")
        assertEquals(listOf(null, "on-host", "removed-model"), choices.map { it.id })
        assertEquals("Default (recommended)", choices.first().name)
        assertNull(catalog.normalize("default-id"))
        assertEquals("removed-model", catalog.normalize("removed-model"))
        assertEquals("Default · Host model", catalog.copy(models = catalog.models.drop(1)).options(null).first().name)
        assertEquals(listOf(ModelChoice(null, "Default")), AgentModels(emptyList()).options(null))
    }

    @Test fun `default model shows the settings of the model the agent runs by default`() {
        val catalog = WireJson.decodeFromString<AgentModels>("""{"models":[{"id":"gpt-6.1-sol","name":"6.1 Sol","options":[{"id":"reasoning_effort","name":"Reasoning Effort","default":"low","values":[{"id":"low","name":"Low"},{"id":"high","name":"High"}]}]},{"id":"gpt-6.1-mini","name":"6.1 Mini","options":[]}],"currentModelId":"gpt-6.1-sol"}""")
        assertEquals(listOf("reasoning_effort"), catalog.settings("").map { it.id })
        assertTrue(catalog.settings("gpt-6.1-mini").isEmpty())
        assertTrue(catalog.settings("typed-by-hand").isEmpty())
        assertTrue(AgentModels(emptyList()).settings("").isEmpty())
    }

    @Test fun `new bot defaults preserve host connectors and a nullable notification toggle turns off`() {
        val draft = BotDraft(command = "stale custom command")
        assertFalse("an unnamed bot is only created, never saved", draft.isValid)
        assertTrue(draft.copy(name = "Scout").isValid)
        // Left unset, the host turns on every connector and skill.
        assertEquals(JsonNull, draft.body()["connectors"])
        assertEquals(JsonNull, draft.body()["skills"])
        assertEquals(JsonNull, draft.body()["command"])
        // An unset switch is left out: the host's booleans don't take null.
        assertFalse("fastMode" in draft.body() || "notify" in draft.body())
        assertFalse(draft.toggleNotifications().notify!!)
        assertTrue(draft.toggleNotifications().toggleNotifications().notify!!)
        assertFalse(draft.copy(id = "existing", name = "  ").isValid)
        assertEquals("private-agent", draft.copy(backend = "custom", command = "private-agent").body()["command"]?.jsonPrimitive?.content)
    }
}
