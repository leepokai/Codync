package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.design.*

/** Another way through setup, as a card holding one row: its symbol, what it does, and a chevron (iOS `SetupChoiceButton`). */
@Composable internal fun SetupChoiceButton(systemImage: String, title: String, detail: String, action: () -> Unit) = CardSection {
    row {
        Row(Modifier.fillMaxWidth().plainButton(action = action), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(Modifier.size(40.dp), contentAlignment = Alignment.Center) { Symbol(systemImage, size = 22.dp, tint = Palette.text) }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(tr(title), style = Type.body.semibold, color = Palette.text)
                Text(tr(detail), style = Type.subheadline, color = Palette.secondary)
            }
            Symbol("chevron.right", size = 14.dp, tint = Palette.tertiary)
        }
    }
}

/** First run, signed in with Codync Cloud open to the account: start a cloud computer instead of installing Codync
 * (New cloud computer, docs/plans/codync-cloud.md; iOS `StartCloudButton`). */
@Composable internal fun StartCloudButton(action: () -> Unit) =
    SetupChoiceButton("cloud", "Start a cloud computer", "No computer? Run one on your own cloud provider account.", action)
