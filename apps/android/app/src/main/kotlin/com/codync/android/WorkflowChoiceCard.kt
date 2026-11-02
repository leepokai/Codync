package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** The step row used by the new-workflow form and the editor. */
@Composable internal fun WorkflowChoiceCard(symbol: String, title: String, detail: String, chevron: Boolean = true,
    translateTitle: Boolean = true, number: Int? = null, parents: List<Int> = emptyList(), caption: String? = null,
    modifier: Modifier = Modifier) {
    Row(modifier.clip(RoundedRectangle(16.dp)).background(Palette.surface).padding(14.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (number != null) Text(number.toString(), style = Type.body.semibold.monospacedDigit, color = Palette.text)
        Box(Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).background(Palette.background), contentAlignment = Alignment.Center) {
            Symbol(symbol, size = 16.dp, tint = Palette.text)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(if (translateTitle) tr(title) else title, style = Type.body.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (detail.isNotBlank()) Text(tr(detail), style = Type.caption, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (parents.isNotEmpty()) StepSources(parents)
            if (!caption.isNullOrBlank()) Text(tr(caption), style = Type.caption, color = Palette.secondary, maxLines = 2)
        }
        if (chevron) Symbol("chevron.right", size = 12.dp, tint = Palette.tertiary)
    }
}

/** The full-width add control under a column of steps. */
@Composable internal fun WorkflowAddStepButton(action: () -> Unit) {
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.surface).plainButton(action = action).padding(vertical = 14.dp),
        horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
        Symbol("plus", size = 16.dp, tint = Palette.text)
        Text(tr("Add step"), Modifier.padding(start = 8.dp), style = Type.body.medium, color = Palette.text)
    }
}
