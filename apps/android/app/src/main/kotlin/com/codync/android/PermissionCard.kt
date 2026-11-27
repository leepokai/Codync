package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.Entry
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * Under what a thread put in the chat (its approval or choice card, a file, a voice note, an artifact): that it comes
 * from the bot's work, and which task. Nothing for the chat's own.
 */
@Composable internal fun WorkThreadLine(entry: Entry, modifier: Modifier = Modifier) {
    if (LocalInThread.current) return
    val title = (entry.data["thread"] as? JsonObject)?.let { (it["title"] as? JsonPrimitive)?.contentOrNull }?.takeIf(String::isNotEmpty) ?: return
    // A clock leads it, as the computer leads "Runs on" (iOS `Label(…, systemImage: "clock")`).
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Symbol("clock", size = 14.dp, tint = Palette.tertiary)
        Text(tr("Work") + " · " + title, style = Type.caption, color = Palette.tertiary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Rows drawn inside a thread's own chat (iOS `workThread` in the environment). */
internal val LocalInThread = androidx.compose.runtime.compositionLocalOf { false }

/** One of the bot's threads put [this] in the chat (iOS `EntryData.isFromThread`): it names its thread's task, by title or by id. */
internal val Entry.isFromThread: Boolean
    get() = !((data["thread"] as? JsonObject)?.get("title") as? JsonPrimitive)?.contentOrNull.isNullOrEmpty() || !(data["task"] as? JsonPrimitive)?.contentOrNull.isNullOrEmpty()

/** A choice card takes the user's own words (iOS `EntryData.takesOwnWords`): not a thread's, nor one only an option
 *  answers (`ownWords: false`, an agent's own question). */
internal val Entry.takesOwnWords: Boolean
    get() = !isFromThread && (data["ownWords"] as? JsonPrimitive)?.booleanOrNull != false

/**
 * A stub never draws Allow or Deny. [MirrorDatabase.answerable] replaces the stub with the full row
 * once that row has been read and fits; only that row carries `command`, `detail` and `diffs`.
 * A pending status and options kept in the head are not the request, and a row that could not be
 * read stays the warning card. Nothing here copies a field off a tail.
 */
internal fun canAnswerStub(entry: Entry): Boolean {
    if (!entry.isStoredStub || entry.approvalReadFailed) return false
    return false
}

/** Approval card in the choice card's form: what the agent wants, where it runs, and the answers as the choice card's
 *  numbered rows, in one tone. */
@Composable internal fun PermissionCard(entry: Entry, hostName: String, answering: Boolean, respond: (String?) -> Unit) {
    // Allow and Deny wait until the full row replaces this stub. Until then, the placeholder.
    if (entry.isStoredStub && !canAnswerStub(entry)) return StoredApproval(entry, hostName)
    val d = entry.data
    val pending = d.text("status") == "pending"
    var expanded by remember { mutableStateOf(false) }
    var chosen by remember { mutableStateOf<String?>(null) }
    val headline = approvalHeadline(d.text("toolKind"))
    val diffs = (d["diffs"] as? JsonArray)?.mapNotNull { it as? JsonObject }.orEmpty()
    val hasDetail = !d.text("command").isNullOrEmpty() || !d.text("detail").isNullOrEmpty() || diffs.isNotEmpty()
    Column(Modifier.padding(end = 40.dp).widthIn(max = 420.dp).clip(RoundedRectangle(22.dp)).background(Palette.bubbleAgent).padding(vertical = 16.dp, horizontal = 8.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)) {
        // The rows run 8dp wider than the rest, so their numbers line up under the text, as on the choice card.
        // The choice card's form: the question, where it runs with Details, the request in a well.
        Column(Modifier.padding(horizontal = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(tr(headline), style = Type.headline, color = Palette.text)
            val path = d.text("cwd")?.let { " · " + it.trimEnd('/').substringAfterLast('/') } ?: ""
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(tr("Runs on {name}{path}").replace("{name}", hostName).replace("{path}", path), Modifier.weight(1f),
                    style = Type.caption, color = Palette.tertiary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (hasDetail) {
                    val turn by animateFloatAsState(if (expanded) 90f else 0f, Motion.layout(), label = "Chevron")
                    Row(Modifier.clickable(role = Role.Button) { expanded = !expanded }, verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(tr("Details"), style = Type.caption.medium, color = Palette.secondary)
                        Symbol("chevron.right", Modifier.rotate(turn), size = 10.dp, tint = Palette.secondary)
                    }
                }
            }
            Text(d.text("title").orEmpty(), Modifier.padding(top = 4.dp).fillMaxWidth().clip(RoundedRectangle(10.dp))
                .background(Palette.text.copy(alpha = 0.06f)).padding(horizontal = 10.dp, vertical = 8.dp),
                style = Type.subheadline.monospaced, color = Palette.secondary, maxLines = if (expanded) Int.MAX_VALUE else 3)
        }
        WorkThreadLine(entry, Modifier.padding(horizontal = 8.dp))
        AnimatedVisibility(hasDetail && expanded, Modifier.padding(horizontal = 8.dp), enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
            exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                d.text("command")?.takeIf(String::isNotEmpty)?.let { CodeBox(it) }
                d.text("detail")?.takeIf(String::isNotEmpty)?.let { CodeBox(it.take(2000)) }
                for (diff in diffs) DiffView(diff)
            }
        }
        if (pending) {
            val rank = mapOf("allow_once" to 0, "allow_always" to 1, "reject_once" to 2, "reject_always" to 3)
            val options = (d["options"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }
                .sortedBy { rank[it.text("kind")] ?: 9 }
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                for ((index, option) in options.withIndex()) {
                    val id = option.text("optionId")?.takeIf(String::isNotBlank) ?: continue
                    val kind = option.text("kind").orEmpty()
                    val waiting = answering && chosen != null
                    val (source, pressed) = rememberPress()
                    Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).clip(RoundedRectangle(12.dp))
                        .background(Palette.text.copy(alpha = if (pressed) 0.08f else 0f))
                        .clickable(source, null, enabled = !answering, role = Role.Button) { chosen = id; respond(id) }
                        .graphicsLayer { alpha = if (!waiting || chosen == id) 1f else 0.4f }
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        RowNumber {
                            if (answering && chosen == id) Spinner(12.dp)
                            else Text("${index + 1}", style = Type.caption.semibold.copy(fontFeatureSettings = "tnum"), color = Palette.secondary)
                        }
                        Text(when (kind) {
                            "allow_once" -> tr("Allow once"); "allow_always" -> tr("Always allow"); "reject_once" -> tr("Deny"); "reject_always" -> tr("Never")
                            else -> option.text("name").orEmpty()
                        }, Modifier.weight(1f), style = Type.body, color = Palette.text)
                    }
                }
            }
        } else Text(outcome(d), Modifier.padding(horizontal = 8.dp), style = Type.caption.medium, color = Palette.secondary)
    }
}

/**
 * An approval the window could not keep whole: its row is gone from this phone, so its answers aren't here. It says
 * that instead of claiming the request expired, which the card would otherwise read from the missing status.
 */
@Composable private fun StoredApproval(entry: Entry, hostName: String) {
    Column(Modifier.padding(end = 40.dp).widthIn(max = 420.dp).clip(RoundedRectangle(22.dp)).background(Palette.bubbleAgent)
        .padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(tr(approvalHeadline(entry.data.text("toolKind"))), style = Type.headline, color = Palette.text)
        Text(tr("Runs on {name}{path}").replace("{name}", hostName).replace("{path}", ""), style = Type.caption, color = Palette.tertiary)
        if (entry.approvalReadFailed) {
            Text(tr("This approval couldn't be read on this phone. Check the computer to answer it."),
                style = Type.subheadline, color = Palette.danger)
        } else {
            Text(entry.stubLabel(), style = Type.subheadline, color = Palette.secondary)
        }
    }
}

/**
 * The sentence a stub shows. The stored text is an id; the count sits beside it. A row that is not
 * that id keeps its own text, which is the message and is not translated.
 */
@Composable internal fun Entry.stubLabel(): String {
    val sentence = storedStubSentence() ?: return text
    val count = (data["storedChars"] as? JsonPrimitive)?.contentOrNull ?: return text
    return tr(STORED_STUB_CAPTION).replace("{count}", count)
}

/** What an approval asks for, in Codync's words. */
internal fun approvalHeadline(kind: String?): String = when (kind) {
    "execute" -> "Wants to run a command"
    "edit", "delete", "move" -> "Wants to change files"
    "fetch" -> "Wants to access the web"
    "read", "search" -> "Wants to read files"
    else -> "Wants to use a tool"
}

/** The answer given, in Codync's words for the usual kinds; an agent's own option keeps its name. */
@Composable private fun outcome(d: JsonObject): String = when (d.text("status")) {
    "answered" -> {
        val chosen = (d["options"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }.firstOrNull { it.text("optionId") == d.text("selected") }
        when (chosen?.text("kind")) {
            "allow_once" -> tr("Allowed once"); "allow_always" -> tr("Always allowed"); "reject_once" -> tr("Denied"); "reject_always" -> tr("Never allowed")
            else -> chosen?.text("name") ?: tr("Answered")
        }
    }
    "cancelled" -> tr("Cancelled")
    else -> tr("Expired — the agent moved on")
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
