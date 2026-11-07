package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.design.*
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive

/** A bot asking the people of a group a short question; they answer by replying to it. */
@Composable internal fun MemberQuestionCard(entry: Entry, state: AppState, reply: (() -> Unit)?) {
    val question = entry.data["memberQuestion"] as? JsonObject ?: return
    val status = question["status"]?.jsonPrimitive?.contentOrNull.orEmpty()
    val answers = (question["answers"] as? JsonArray)?.mapNotNull { it.jsonPrimitive.contentOrNull }.orEmpty()
    val author = entry.data.text("author")?.let { id -> state.bots.firstOrNull { it.id == id }?.name }
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(Palette.bubbleAgent).padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp)) {
        IconLabel("${author ?: tr("A bot")} · ${tr("Needs a member")}", "person.2", Palette.secondary, style = Type.caption)
        Text(entry.text, style = Type.body, color = Palette.text)
        for (answer in answers) Text("“$answer”", style = Type.callout, color = Palette.secondary)
        if (status == "pending" && reply != null) SecondaryButton(tr("Answer"), enabled = true, action = reply)
    }
}
