package com.codync.android

import android.util.TypedValue
import android.view.ActionMode
import android.view.Menu
import android.widget.TextView
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.codync.android.design.ModalHeader
import com.codync.android.design.Palette
import com.codync.android.design.Type
import com.codync.android.design.tr
import kotlin.math.roundToInt

/** Select Text: a message's text as written, to select a part of and reply to it (`onReply`, with that part). */
@Composable internal fun SelectTextSheet(text: String, onReply: (String) -> Unit) {
    Column {
        ModalHeader("Select Text")
        Box(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).padding(bottom = 24.dp)) {
            SelectableText(text, Modifier.fillMaxWidth().semantics { testTagsAsResourceId = true }.testTag("select-text"), onReply)
        }
    }
}

/**
 * Body text the user can select, with Reply beside the system's Copy and Select all in the selection toolbar. A
 * platform `TextView`: Compose's `SelectionContainer` can't say what is selected.
 */
@Composable private fun SelectableText(text: String, modifier: Modifier, onReply: (String) -> Unit) {
    val reply by rememberUpdatedState(onReply)
    val title = tr("Reply")
    val ink = Palette.text.toArgb()
    val highlight = Palette.accentDim.toArgb()
    val style = Type.body
    AndroidView(modifier = modifier, factory = { context ->
        TextView(context).apply {
            setTextIsSelectable(true)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, style.fontSize.value)
            lineHeight = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, style.lineHeight.value, resources.displayMetrics).roundToInt()
            letterSpacing = style.letterSpacing.value
            // Inter at the body's optical size, as `Type` sets it.
            typeface = resources.getFont(com.codync.android.design.R.font.inter)
            fontVariationSettings = "'wght' 400, 'opsz' ${style.fontSize.value}"
        }
    }, update = { view ->
        if (view.text.toString() != text) view.text = text
        view.setTextColor(ink)
        view.highlightColor = highlight
        view.customSelectionActionModeCallback = ReplyToSelection(view, title) { reply(it) }
    })
}

/** Adds Reply to a text view's selection toolbar; choosing it hands over the selected text. */
private class ReplyToSelection(private val view: TextView, private val title: String, private val onReply: (String) -> Unit) : ActionMode.Callback {
    override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
        menu.add(Menu.NONE, REPLY, 0, title)
        return true
    }

    override fun onPrepareActionMode(mode: ActionMode, menu: Menu) = false

    override fun onActionItemClicked(mode: ActionMode, item: android.view.MenuItem): Boolean {
        if (item.itemId != REPLY) return false
        val start = minOf(view.selectionStart, view.selectionEnd).coerceAtLeast(0)
        val selected = view.text.subSequence(start, maxOf(view.selectionStart, view.selectionEnd, start)).toString()
        mode.finish()
        onReply(selected)
        return true
    }

    override fun onDestroyActionMode(mode: ActionMode) = Unit

    private companion object {
        const val REPLY = 1
    }
}
