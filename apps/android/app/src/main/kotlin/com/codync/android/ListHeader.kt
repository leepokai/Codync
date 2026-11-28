package com.codync.android

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import com.kyant.shapes.Capsule

// The pieces a screen's header shares (the chat, and the Bots, Threads and Workflows tabs).

/**
 * A list tab's header (Bots, Threads, Workflows): the account, the [title] over its quiet [subtitle] (iOS `navigationTitle`
 * + `navigationSubtitle`), then the tab's [actions]; while [searching], the search field and Cancel, which [close]s it.
 */
@Composable internal fun ListHeader(app: AccountContext, account: () -> Unit, title: String, subtitle: String, searching: Boolean, query: String,
    setQuery: (String) -> Unit, close: () -> Unit, modifier: Modifier = Modifier, prompt: String = "Search", actions: @Composable RowScope.() -> Unit) {
    Row(modifier.fillMaxWidth().padding(top = 6.dp).padding(horizontal = 16.dp).height(44.dp), verticalAlignment = Alignment.CenterVertically) {
        if (searching) ListSearchField(query, setQuery, prompt, close)
        else {
            AccountSwitcherButton(app, account)
            ListTitle(title, subtitle)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), content = actions)
        }
    }
}

@Composable private fun RowScope.ListTitle(title: String, subtitle: String) {
    Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
        Text(tr(title), Modifier.semantics { heading() }, style = Type.body.semibold, color = Palette.text, maxLines = 1)
        Text(subtitle, style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** A pushed page's glass Back, as the chat's header has it. */
@Composable internal fun GlassBackButton(action: () -> Unit) = HeaderButton("chevron.left", "Back", symbolSize = 32.dp, action = action)

/** A glass button in a header. */
@Composable internal fun HeaderButton(symbol: String, title: String, modifier: Modifier = Modifier, symbolModifier: Modifier = Modifier,
    symbolSize: Dp = 22.dp, action: () -> Unit) {
    val (source, pressed) = rememberPress()
    val label = tr(title)
    Box(modifier.size(44.dp).glass(Capsule(), pressed = pressed)
        .clickable(source, null, role = Role.Button, onClick = action).semantics { contentDescription = label },
        contentAlignment = Alignment.Center) {
        Symbol(symbol, symbolModifier, size = symbolSize, tint = Palette.text)
    }
}

/** A list header's open search: the field, then Cancel, which [close]s it. */
@Composable private fun RowScope.ListSearchField(query: String, change: (String) -> Unit, prompt: String, close: () -> Unit) {
    SearchField(query, change, Modifier.weight(1f), prompt = prompt, autoFocus = true)
    Text(tr("Cancel"), Modifier.padding(start = 12.dp).plainButton(action = close), style = Type.body, color = Palette.text)
}
