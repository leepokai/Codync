package com.codync.android

import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import com.codync.android.core.AgentModels
import com.codync.android.core.BotDraft
import com.codync.android.core.Personality
import com.codync.android.core.PersonalityLayer
import com.codync.android.design.*

/** One personality field of the editor: its label, help, hint and the layer it edits. */
private data class EditorLayer(val title: String, val detail: String, val hint: String, val layer: PersonalityLayer)

private val LAYERS = listOf(
    EditorLayer("Soul", "Values and boundaries that never change. Written by you; the bot can't edit it.", "e.g. Honest, careful with other people's data.", PersonalityLayer.Soul),
    EditorLayer("Persona", "Its voice and manner.", "e.g. Dry and brief. No emoji.", PersonalityLayer.Persona),
    EditorLayer("Heartbeat", "What it does when it checks in on its own.", "e.g. Look for failing tests and say only if something broke.", PersonalityLayer.Heartbeat),
)

/** What the user writes about how a bot works: its check before done, its soul, persona and heartbeat, its background model. */
internal fun CardRows.characterRows(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit) {
    row {
        Field("Check before done", "A command run in the workspace after each turn. If it fails, the turn doesn't count as done.") {
            FieldBox(draft.verifyCommand.orEmpty(), { change(draft.copy(verifyCommand = it)) }, "e.g. npm test", 1, 3)
        }
    }
    for (layer in LAYERS) row {
        val current = draft.personality ?: Personality()
        Field(layer.title, layer.detail) { FieldBox(layer.layer.of(current), { change(draft.copy(personality = layer.layer.set(current, it))) }, layer.hint, 2, 8) }
    }
    row {
        // The agent's own models; a model that is set but not listed (another agent, an older host) stays pickable.
        var models by remember(state.computers.firstOrNull()?.id, draft.backend, draft.account) { mutableStateOf<AgentModels?>(null) }
        LaunchedEffect(state.computers.firstOrNull()?.id, draft.backend, draft.account) {
            models = if (draft.backend == "custom") null else runCatching { store.agentModels(draft.backend, draft.account) }.getOrNull()
        }
        val chosen = draft.backgroundModel.orEmpty()
        val listed = models?.models.orEmpty().map { it.id to it.name }
        val choices = listOf("" to "The bot's own model") + listed + (if (chosen.isNotEmpty() && listed.none { it.first == chosen }) listOf(chosen to chosen) else emptyList())
        ValueRow("Background model", detail = "A cheaper model for routines and check-ins, where nobody is waiting.") {
            // Empty is sent as "" so a model that was set can be cleared.
            ChoicePicker(chosen, choices, { change(draft.copy(backgroundModel = it)) }, label = "Background model")
        }
    }
    if (draft.backgroundModel.orEmpty().isNotBlank() && state.bots.any { it.saver != null }) row {
        SwitchRow("Saver", "At 80 percent of this bot's daily token budget, new sessions start on the background model until tomorrow.",
            draft.saver ?: false, { change(draft.copy(saver = it)) })
    }
}
