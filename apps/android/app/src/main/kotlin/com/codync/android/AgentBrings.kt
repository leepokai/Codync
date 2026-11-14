package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** One skill, connector, rule or plugin the agent keeps for itself (`agentGear`). */
internal data class GearItem(val name: String, val description: String, val detail: String)

internal data class AgentGear(
    val skills: List<GearItem>,
    val connectors: List<GearItem>,
    val rules: List<GearItem> = emptyList(),
    val plugins: List<GearItem> = emptyList(),
) {
    val shows get() = skills.isNotEmpty() || connectors.isNotEmpty() || rules.isNotEmpty() || plugins.isNotEmpty()
}

/** What `backend` already has in the user's home. Codync's own skills and connectors are not in this list. */
internal suspend fun AppStore.agentGear(backend: String): AgentGear {
    val body = hostCall("agentGear", buildJsonObject { put("backend", backend) })
    val obj = body as? JsonObject ?: return AgentGear(emptyList(), emptyList())
    return AgentGear(obj.gearItems("skills"), obj.gearItems("connectors"), obj.gearItems("rules"), obj.gearItems("plugins"))
}

private fun JsonObject.gearItems(key: String): List<GearItem> {
    val rows = this[key] as? JsonArray ?: return emptyList()
    return rows.mapNotNull { row ->
        val item = row as? JsonObject ?: return@mapNotNull null
        val name = item["name"]?.jsonPrimitive?.contentOrNull?.trim().orEmpty()
        if (name.isEmpty()) return@mapNotNull null
        GearItem(name, item["description"]?.jsonPrimitive?.contentOrNull.orEmpty(), item["detail"]?.jsonPrimitive?.contentOrNull.orEmpty())
    }
}

/** Skills, connectors, rules and plugins the agent already has, under Sign in on its sheet. */
@Composable internal fun AgentBrings(backendId: String, store: AppStore, online: Boolean) {
    var gear by remember(backendId) { mutableStateOf<AgentGear?>(null) }
    var failed by remember(backendId) { mutableStateOf(false) }
    var open by remember(backendId) { mutableStateOf<String?>(null) }
    // Loaded again when the computer comes back online, so a failed load doesn't stay.
    LaunchedEffect(backendId, online) {
        if (!online) return@LaunchedEffect
        try {
            gear = store.agentGear(backendId)
            failed = false
        } catch (failure: Exception) {
            if (failure is kotlinx.coroutines.CancellationException) throw failure
            failed = true
        }
    }
    when {
        failed -> CardSection { row { Text(tr("Couldn't load what this agent brings."), style = Type.footnote, color = Palette.danger) } }
        gear?.shows == true -> {
            val loaded = gear ?: return
            val groups = listOf("Skills" to loaded.skills, "Connectors" to loaded.connectors, "Rules" to loaded.rules, "Plugins" to loaded.plugins)
            CardSection("What the agent brings") {
                for ((title, items) in groups) {
                    if (items.isEmpty()) continue
                    row {
                        Disclosure(open == title, { open = if (open == title) null else title }, label = {
                            Box(Modifier.size(26.dp).clip(CircleShape).background(Palette.bubbleAgent), contentAlignment = Alignment.Center) {
                                Symbol(icon(title), size = 13.dp, tint = Palette.text)
                            }
                            Text(tr(title), Modifier.weight(1f).padding(start = 10.dp), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text("${items.size}", Modifier.clip(CircleShape).background(Palette.bubbleAgent).padding(horizontal = 7.dp, vertical = 2.dp),
                                style = Type.caption.semibold.monospacedDigit, color = Palette.secondary)
                        }) {
                            Column(Modifier.fillMaxWidth().padding(top = 6.dp)) {
                                items.forEachIndexed { index, item ->
                                    if (index > 0) Box(Modifier.padding(start = 36.dp).fillMaxWidth().height(0.5.dp).background(Palette.border))
                                    val note = item.description.ifEmpty { item.detail }
                                    Column(Modifier.fillMaxWidth().padding(start = 36.dp, top = 8.dp, bottom = 8.dp)) {
                                        Text(item.name, style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                        if (note.isNotEmpty()) Text(note, style = Type.caption, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

private fun icon(title: String) = when (title) {
    "Skills" -> "book.pages"
    "Connectors" -> "point.3.connected.trianglepath.dotted"
    "Rules" -> "list.bullet.rectangle"
    else -> "square.grid.2x2"
}
