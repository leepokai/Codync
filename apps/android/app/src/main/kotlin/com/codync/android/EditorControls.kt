package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.FlowRowScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle

// The settings-form pieces of iOS `BotEditorView`: labelled fields, switch rows, the avatar
// picker (also on New bot) and the connector/skill toggles.

/** A labeled input inside a section row: label (and an optional note) over the field. */
@Composable internal fun Field(label: String, detail: String? = null, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(tr(label), style = Type.body, color = Palette.text)
            if (detail != null) Text(tr(detail), style = Type.caption.copy(lineHeight = Type.caption.lineHeight * 1.12f), color = Palette.secondary)
        }
        content()
    }
}

/** A rounded input box (`fieldBox`): a plain field on the ground color, or on the card color in an outlined card. */
@Composable internal fun FieldBox(text: String, onChange: (String) -> Unit, prompt: String, minLines: Int = 1, maxLines: Int = 1,
    style: TextStyle = Type.body, fill: Color = if (LocalCardStyle.current == CardStyle.Outlined) Palette.surface else Palette.background,
    capitalize: Boolean = true, imeAction: ImeAction = ImeAction.Default,
    actions: KeyboardActions = KeyboardActions.Default) {
    Box(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(fill).padding(horizontal = 14.dp, vertical = 11.dp)) {
        PlainTextField(text, onChange, Modifier.fillMaxWidth(), prompt, singleLine = maxLines == 1, style = style, minLines = minLines,
            maxLines = maxLines, keyboard = KeyboardOptions(capitalization = if (capitalize) KeyboardCapitalization.Sentences else KeyboardCapitalization.None,
                autoCorrectEnabled = capitalize, imeAction = imeAction), actions = actions)
    }
}

/** An on/off row: title and a short note, the switch on the right. */
@Composable internal fun SwitchRow(title: String, detail: String? = null, isOn: Boolean, onChange: (Boolean) -> Unit, enabled: Boolean = true) {
    CodyncSwitch(isOn, onChange, Modifier.fillMaxWidth(), enabled) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(tr(title), style = Type.body, color = Palette.text)
            if (detail != null) Text(tr(detail), style = Type.caption.copy(lineHeight = Type.caption.lineHeight * 1.12f), color = Palette.secondary)
        }
    }
}

/** The bot's face: every shape over every color, as wrapping centered grids, each ringed when chosen. */
@Composable internal fun AvatarPicker(shape: String, color: String, chooseShape: (String) -> Unit, chooseColor: (String) -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ChoiceGrid {
            for (option in AvatarPalette.shapes) Box(Modifier.choice(option == shape, "$option shape") { chooseShape(option) }.ring(option == shape)) {
                CharacterAvatar(option, color, 40.dp)
            }
        }
        ChoiceGrid {
            for ((id, hex) in AvatarPalette.colors) Box(Modifier.choice(id == color, id.replaceFirstChar(Char::uppercase)) { chooseColor(id) }.ring(id == color)) {
                Box(Modifier.size(28.dp).clip(Capsule()).background(Color(hex)))
            }
        }
    }
}

/** A grid that wraps onto centered rows, for the avatar and member choices. */
@Composable internal fun ChoiceGrid(spacing: Dp = 10.dp, content: @Composable FlowRowScope.() -> Unit) =
    FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(spacing, Alignment.CenterHorizontally),
        verticalArrangement = Arrangement.spacedBy(spacing), content = content)

/** A tappable choice in a [ChoiceGrid]: dims while pressed; put the [ring] on what shows it's chosen. */
@Composable internal fun Modifier.choice(chosen: Boolean, label: String, role: Role = Role.RadioButton, choose: () -> Unit) =
    plainButton(role = role, action = choose).semantics { contentDescription = label; selected = chosen }

/** The accent ring around a chosen shape, color or member (iOS `strokeBorder(Palette.accent)`). */
@Composable internal fun Modifier.ring(chosen: Boolean) = clip(Capsule()).border(2.dp, if (chosen) Palette.accent else Color.Transparent, Capsule()).padding(4.dp)

/** A card of on/off switches for the connectors or skills installed on the computer. */
@Composable internal fun PluginToggles(title: String, empty: String, items: List<Triple<String, String, String>>, selection: List<String>,
    change: (List<String>) -> Unit, info: ((String) -> Unit)? = null) {
    CardSection(title) {
        if (items.isEmpty()) row { Text(tr(empty), style = Type.body, color = Palette.secondary) }
        for ((id, name, detail) in items) row {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Box(Modifier.weight(1f)) {
                    SwitchRow(name, detail.ifEmpty { null }, id in selection, { on -> change(selection.filter { it != id } + if (on) listOf(id) else emptyList()) })
                }
                if (info != null) IconButton("About $name", "info.circle") { info(id) }
            }
        }
    }
}
