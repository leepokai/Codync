package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import com.codync.android.design.*

/**
 * A plain field in a card row (`TextField` / `SecureField` with `.plainTextInput()`): no chrome,
 * no autocorrect, the prompt in the tertiary ink; a secret one hides what's typed.
 */
@Composable internal fun PlainInput(text: String, onChange: (String) -> Unit, prompt: String, modifier: Modifier = Modifier,
    secret: Boolean = false, style: TextStyle = Type.body, minLines: Int = 1, maxLines: Int = 1, enabled: Boolean = true) {
    val hint = tr(prompt)
    BasicTextField(text, onChange, modifier.fillMaxWidth().semantics { contentDescription = hint }, enabled = enabled,
        singleLine = maxLines == 1, minLines = minLines, maxLines = maxLines, textStyle = style.copy(color = Palette.text),
        cursorBrush = SolidColor(Palette.accent),
        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false,
            keyboardType = if (secret) KeyboardType.Password else KeyboardType.Text),
        decorationBox = { field ->
            Box(contentAlignment = Alignment.CenterStart) {
                if (text.isEmpty()) Text(hint, style = style, color = Palette.tertiary, maxLines = maxLines)
                field()
            }
        })
}

/** A small caption over a field (iOS `CredentialFieldRow`). */
@Composable internal fun FieldRow(title: String, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(tr(title), style = Type.caption, color = Palette.secondary)
        content()
    }
}

/** A field with its note under it, the iOS setup forms' row. */
@Composable internal fun NotedInput(text: String, onChange: (String) -> Unit, prompt: String, note: String?, secret: Boolean = false) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        PlainInput(text, onChange, prompt, secret = secret)
        if (note != null) Text(tr(note), style = Type.caption, color = Palette.secondary)
    }
}

/** An error inside a card. */
@Composable internal fun ErrorRow(text: String) = Text(tr(text), style = Type.body, color = Palette.danger)
