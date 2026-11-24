package com.codync.android

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import com.kyant.shapes.Capsule

// The pieces a screen's header shares (the chat, and the Bots and Threads tabs).

/** A glass button in a header. */
@Composable internal fun HeaderButton(symbol: String, title: String, modifier: Modifier = Modifier, symbolModifier: Modifier = Modifier,
    action: () -> Unit) {
    val (source, pressed) = rememberPress()
    val label = tr(title)
    Box(modifier.size(44.dp).glass(Capsule(), pressed = pressed)
        .clickable(source, null, role = Role.Button, onClick = action).semantics { contentDescription = label },
        contentAlignment = Alignment.Center) {
        Symbol(symbol, symbolModifier, size = 22.dp, tint = Palette.text)
    }
}

/** A list header's open search (the Bots and Threads tabs): the field, then Cancel, which [close]s it. */
@Composable internal fun RowScope.ListSearchField(query: String, change: (String) -> Unit, prompt: String, close: () -> Unit) {
    SearchField(query, change, Modifier.weight(1f), prompt = prompt, autoFocus = true)
    Text(tr("Cancel"), Modifier.padding(start = 12.dp).plainButton(action = close), style = Type.body, color = Palette.text)
}
