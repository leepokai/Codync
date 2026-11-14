package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.codync.android.core.TimeZones
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** The routine's time zone as a field: the zone and its offset; tapping opens `ZonePicker`. */
@Composable internal fun ZoneButton(zone: String, open: () -> Unit) {
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.bubbleUser).plainButton(action = open)
        .semantics(mergeDescendants = true) { contentDescription = "Time zone: $zone" }.padding(horizontal = 14.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(zone.ifEmpty { "Choose" }, Modifier.weight(1f), style = Type.body, color = Palette.text, maxLines = 1)
        Text(TimeZones.offset(zone), style = Type.body, color = Palette.secondary)
        Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
    }
}

/** Every IANA zone with its offset, searched as you type; the chosen one is checked. */
@Composable internal fun ZonePicker(selection: String, pick: (String) -> Unit) {
    var query by remember { mutableStateOf("") }
    val zones = remember(query) { TimeZones.search(query) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Time zone")
        SearchField(query, { query = it }, Modifier.padding(horizontal = 20.dp), "Search time zones")
        if (zones.isEmpty()) Text("No time zones match “${query.trim()}”.", Modifier.padding(24.dp), style = Type.body, color = Palette.secondary)
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(start = 24.dp, end = 24.dp, top = 12.dp, bottom = 36.dp)) {
            items(zones, key = { it }) { zone ->
                Row(Modifier.fillMaxWidth().selectable(zone == selection, role = Role.RadioButton) { pick(zone) }.padding(vertical = 11.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(zone, Modifier.weight(1f), style = Type.body, color = Palette.text)
                    Text(TimeZones.offset(zone), style = Type.subheadline, color = Palette.secondary)
                    Symbol("checkmark", size = 15.dp, tint = Palette.text, modifier = Modifier.graphicsLayer { alpha = if (zone == selection) 1f else 0f })
                }
            }
        }
    }
}
