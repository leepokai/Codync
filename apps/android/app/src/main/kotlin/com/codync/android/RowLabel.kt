package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.codync.android.design.*

/** An icon and a short name, with a chevron when it leads somewhere. */
@Composable internal fun RowLabel(title: String, systemImage: String, tint: Color = Palette.text, chevron: Boolean = false,
    modifier: Modifier = Modifier) {
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.width(26.dp), contentAlignment = Alignment.Center) { Symbol(systemImage, size = 20.dp, tint = tint) }
        Text(tr(title), Modifier.weight(1f), style = Type.body, color = tint)
        if (chevron) Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
    }
}
