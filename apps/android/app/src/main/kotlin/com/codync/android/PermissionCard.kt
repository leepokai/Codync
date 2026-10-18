package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.Entry
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Approval card in Grok Bot's choice-card style: what the agent wants, where it runs, and the answers as rows. */
@Composable internal fun PermissionCard(entry: Entry, hostName: String, answering: Boolean, respond: (String?) -> Unit) {
    val d = entry.data
    val pending = d.text("status") == "pending"
    var expanded by remember { mutableStateOf(false) }
    var chosen by remember { mutableStateOf<String?>(null) }
    val headline = when (d.text("toolKind")) {
        "execute" -> "Wants to run a command"
        "edit", "delete", "move" -> "Wants to change files"
        "fetch" -> "Wants to access the web"
        "read", "search" -> "Wants to read files"
        else -> "Wants to use a tool"
    }
    val diffs = (d["diffs"] as? JsonArray)?.mapNotNull { it as? JsonObject }.orEmpty()
    val hasDetail = !d.text("command").isNullOrEmpty() || !d.text("detail").isNullOrEmpty() || diffs.isNotEmpty()
    Column(Modifier.padding(end = 40.dp).clip(RoundedRectangle(22.dp)).background(Palette.bubbleAgent).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(headline, style = Type.headline, color = Palette.text)
            if (pending) Box(Modifier.size(7.dp).clip(Capsule()).background(Palette.warning))
        }
        Text(d.text("title").orEmpty(), style = Type.subheadline.monospaced, color = Palette.secondary, maxLines = if (expanded) Int.MAX_VALUE else 3)
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Symbol("desktopcomputer", size = 14.dp, tint = Palette.tertiary)
            Text("Runs on $hostName" + (d.text("cwd")?.let { " · " + it.trimEnd('/').substringAfterLast('/') } ?: ""),
                style = Type.caption, color = Palette.tertiary)
        }
        if (hasDetail) Disclosure(expanded, { expanded = !expanded }, label = {
            Text("Details", style = Type.caption.medium, color = Palette.secondary)
        }) {
            Column(Modifier.padding(top = 6.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                d.text("command")?.takeIf(String::isNotEmpty)?.let { CodeBox(it) }
                d.text("detail")?.takeIf(String::isNotEmpty)?.let { CodeBox(it.take(2000)) }
                for (diff in diffs) DiffView(diff)
            }
        }
        if (pending) {
            val rank = mapOf("allow_once" to 0, "allow_always" to 1, "reject_once" to 2, "reject_always" to 3)
            val options = (d["options"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }
                .sortedBy { rank[it.text("kind")] ?: 9 }
            Column(Modifier.padding(top = 4.dp).fillMaxWidth().clip(RoundedRectangle(14.dp)).background(Palette.background)) {
                for ((index, option) in options.withIndex()) {
                    val id = option.text("optionId") ?: continue
                    val kind = option.text("kind").orEmpty()
                    if (index > 0) Box(Modifier.fillMaxWidth().height(0.5.dp).background(Palette.border))
                    val waiting = answering && chosen != null
                    Row(Modifier.fillMaxWidth().heightIn(min = 46.dp)
                        .clickable(enabled = !answering, role = Role.Button) { chosen = id; respond(id) }
                        .graphicsLayer { alpha = if (!waiting || chosen == id) 1f else 0.4f }
                        .padding(horizontal = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(when (kind) {
                            "allow_once" -> "Allow once"; "allow_always" -> "Always allow"; "reject_once" -> "Deny"; "reject_always" -> "Never"
                            else -> option.text("name").orEmpty()
                        }, Modifier.weight(1f), style = if (kind == "allow_once") Type.body.semibold else Type.body,
                            color = if (kind.startsWith("allow")) Palette.text else Palette.danger)
                        if (answering && chosen == id) Spinner(14.dp)
                    }
                }
            }
        } else Text(outcome(d), style = Type.caption.medium, color = Palette.secondary)
    }
}

private fun outcome(d: JsonObject): String = when (d.text("status")) {
    "answered" -> {
        val chosen = (d["options"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }.firstOrNull { it.text("optionId") == d.text("selected") }
        when (chosen?.text("kind")) {
            "allow_once" -> "Allowed once"; "allow_always" -> "Always allowed"; "reject_once" -> "Denied"; "reject_always" -> "Never allowed"
            else -> chosen?.text("name") ?: "Answered"
        }
    }
    "cancelled" -> "Cancelled"
    else -> "Expired — the agent moved on"
}

/** One file's change: its name and line counts over the colored patch. */
@Composable internal fun DiffView(diff: JsonObject) {
    val path = diff.text("path").orEmpty()
    val isNew = diff["isNew"]?.jsonPrimitive?.booleanOrNull == true
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Symbol(if (isNew) "doc.badge.plus" else "doc.text", size = 14.dp, tint = Palette.secondary)
            Text(path.substringAfterLast('/'), Modifier.weight(1f), style = Type.caption.monospaced, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("+" + (diff["added"]?.jsonPrimitive?.intOrNull ?: 0), style = Type.caption.monospaced, color = Palette.added)
            Text("−" + (diff["removed"]?.jsonPrimitive?.intOrNull ?: 0), style = Type.caption.monospaced, color = Palette.removed)
        }
        Box(Modifier.fillMaxWidth().heightIn(max = 260.dp).clip(RoundedRectangle(8.dp)).background(Palette.codeBackground)
            .horizontalScroll(rememberScrollState())) {
            Column(Modifier.padding(8.dp)) {
                for (line in diff.text("patch").orEmpty().split("\n").take(80)) Text(line.ifEmpty { " " }, style = Type.caption2.monospaced,
                    color = if (line.startsWith("+")) Palette.added else if (line.startsWith("-")) Palette.removed else Palette.secondary, softWrap = false)
            }
        }
    }
}
