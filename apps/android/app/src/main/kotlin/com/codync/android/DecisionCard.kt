package com.codync.android

import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import com.codync.android.core.DecisionStatus
import com.codync.android.core.Entry
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** Selection and expiry come from the host, including answers made on another device.
 *  The last row sends the user's own words as an ordinary message, which the host takes as the answer. */
@Composable internal fun DecisionCard(entry: Entry, answering: Boolean, respond: (String) -> Unit, reply: (String) -> Unit) {
    val d = entry.data
    val pending = entry.decisionStatus == DecisionStatus.Pending
    val choices = entry.choices
    val suggested = entry.recommended
    var chosen by remember(entry.id) { mutableStateOf<String?>(null) }
    var expanded by remember(entry.id) { mutableStateOf(false) }
    // Whether the clamped question hides lines: only then is there something to expand.
    var clamped by remember(entry.id) { mutableStateOf(false) }
    val question = d.text("question").orEmpty()
    Column(Modifier.semantics { testTagsAsResourceId = true }
        .padding(end = 40.dp).widthIn(max = 420.dp).clip(RoundedRectangle(22.dp))
        .background(Palette.bubbleAgent).animateContentSize(Motion.layoutSize).padding(vertical = 16.dp, horizontal = 8.dp)
        .testTag("decision-card"),
        verticalArrangement = Arrangement.spacedBy(12.dp)) {
        // The rows run 8dp wider than the question and outcome, so their numbers line up under the text.
        Row(Modifier.padding(horizontal = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(question, Modifier.weight(1f), style = Type.headline, color = Palette.text,
                maxLines = if (expanded) Int.MAX_VALUE else 4, onTextLayout = { if (!expanded) clamped = it.hasVisualOverflow })
            if (expanded || clamped) IconButton(if (expanded) "Collapse" else "Expand", if (expanded) "chevron.up" else "chevron.down",
                Modifier.testTag("decision-expand")) { expanded = !expanded }
        }
        if (pending) Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            choices.forEachIndexed { index, choice ->
                val (source, pressed) = rememberPress()
                Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).clip(RoundedRectangle(12.dp))
                    .background(Palette.text.copy(alpha = if (pressed) 0.08f else 0f))
                    .clickable(source, null, enabled = !answering, role = Role.Button) { chosen = choice; respond(choice) }
                    .graphicsLayer { alpha = if (!answering || chosen == choice) 1f else 0.4f }
                    .padding(horizontal = 8.dp, vertical = 4.dp).testTag("decision-option-$index"),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    RowNumber {
                        if (answering && chosen == choice) Spinner(12.dp)
                        else Text("${index + 1}", style = Type.caption.semibold.copy(fontFeatureSettings = "tnum"), color = Palette.secondary)
                    }
                    Text(choice, Modifier.weight(1f), style = if (suggested == index) Type.body.semibold else Type.body, color = Palette.text)
                    if (suggested == index) Text(tr("Suggested"), style = Type.caption, color = Palette.tertiary)
                }
            }
            OwnAnswer(enabled = !answering, reply)
        } else Text(if (entry.decisionStatus == DecisionStatus.Answered) d.text("selected") ?: tr("Answered") else tr("No answer — the bot moved on"),
            Modifier.padding(horizontal = 8.dp).testTag("decision-outcome"), style = Type.caption.medium, color = Palette.secondary)
    }
}

/** The last row: the user's own answer, sent with the keyboard's send key or the send button. A pencil marks it,
 *  not a number: a typed number only ever picks an option. */
@Composable private fun OwnAnswer(enabled: Boolean, reply: (String) -> Unit) {
    var text by remember { mutableStateOf("") }
    var typing by remember { mutableStateOf(false) }
    val focus = LocalFocusManager.current
    val fill by animateFloatAsState(if (typing) 0.06f else 0f, Motion.hover(), label = "own answer fill")
    val send = {
        if (text.isNotBlank()) { reply(text.trim()); text = ""; focus.clearFocus() }
    }
    Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).clip(RoundedRectangle(12.dp))
        .background(Palette.text.copy(alpha = fill))
        .graphicsLayer { alpha = if (enabled) 1f else 0.4f }
        .padding(horizontal = 8.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        RowNumber { Symbol("pencil", size = 12.dp, tint = Palette.secondary) }
        PlainTextField(text, { text = it }, Modifier.weight(1f).onFocusChanged { typing = it.isFocused }.testTag("decision-own"),
            prompt = "Something else", singleLine = true, imeAction = ImeAction.Send, enabled = enabled,
            actions = KeyboardActions(onSend = { send() }))
        if (text.isNotBlank()) IconButton("Send", "arrow.up", Modifier.testTag("decision-own-send"), enabled = enabled) { send() }
    }
}

/** A row's number, or the own answer's pencil. TalkBack reads the row itself. */
@Composable private fun RowNumber(content: @Composable () -> Unit) {
    Box(Modifier.size(24.dp).clip(RoundedRectangle(7.dp)).background(Palette.text.copy(alpha = 0.06f)).clearAndSetSemantics {},
        contentAlignment = Alignment.Center) { content() }
}
