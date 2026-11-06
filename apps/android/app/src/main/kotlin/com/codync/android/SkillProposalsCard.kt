package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp
import com.codync.android.core.SkillProposal
import com.codync.android.design.*
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** The skills a bot proposes; null from an older host. [extra] adds the fields of an edit. */
suspend fun AppStore.skillProposals(method: String, botId: String, id: String? = null, extra: (kotlinx.serialization.json.JsonObjectBuilder.() -> Unit)? = null): List<SkillProposal>? =
    runCatching {
        val response: JsonElement = hostQuery(method, buildJsonObject {
            put("botId", botId)
            id?.let { put("id", it) }
            extra?.invoke(this)
        })
        SkillProposal.decodeAll(response)
    }.getOrNull()

/** One card per proposal; nothing here is a skill until it is approved. */
@Composable internal fun SkillProposalsCard(botId: String, store: AppStore) {
    var proposals by remember(botId) { mutableStateOf<List<SkillProposal>?>(null) }
    var editing by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(botId) { proposals = store.skillProposals("skillProposals", botId) }
    val list = proposals ?: return
    fun act(method: String, id: String, extra: (kotlinx.serialization.json.JsonObjectBuilder.() -> Unit)? = null) {
        scope.launch {
            val next = store.skillProposals(method, botId, id, extra)
            if (next == null) error = "That did not work." else { proposals = next; error = null; editing = null }
        }
    }
    CardSection(SkillProposal.inboxTitle(list.size), footer = "After a task that used several kinds of tool, the bot drafts a skill from it. It stays here until you approve it.") {
        error?.let { row { Text(it, style = Type.caption, color = Palette.danger) } }
        if (list.isEmpty()) row { Text(tr("No proposals."), style = Type.body, color = Palette.secondary) }
        for (p in list) row {
            var name by remember(p.id) { mutableStateOf(p.name) }
            var description by remember(p.id) { mutableStateOf(p.description) }
            var instructions by remember(p.id) { mutableStateOf(p.instructions) }
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                if (editing == p.id) {
                    PlainTextField(name, { name = it }, prompt = "Name")
                    PlainTextField(description, { description = it }, prompt = "When to use it")
                    PlainTextField(instructions, { instructions = it }, prompt = "Steps")
                    SecondaryButton("Save", enabled = name.isNotBlank() && description.isNotBlank() && instructions.isNotBlank()) {
                        act("editSkillProposal", p.id) { put("name", name); put("description", description); put("instructions", instructions) }
                    }
                    SecondaryButton("Cancel") { editing = null }
                } else {
                    Text(p.name, style = Type.body, color = Palette.text)
                    Text(p.description, style = Type.footnote, color = Palette.secondary)
                    for (step in p.stepsPreview()) Text(step, style = Type.caption, color = Palette.tertiary, maxLines = 1)
                    Text("${tr("Used")}: ${p.toolsText()}", style = Type.caption, color = Palette.tertiary)
                    SecondaryButton("Approve", enabled = p.approvable) { act("approveSkillProposal", p.id) }
                    SecondaryButton("Edit") { editing = p.id }
                    SecondaryButton("Dismiss") { act("dismissSkillProposal", p.id) }
                }
            }
        }
    }
}
