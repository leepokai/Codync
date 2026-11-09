package com.codync.android

import android.content.Context
import androidx.compose.foundation.lazy.LazyItemScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.zIndex
import androidx.core.content.edit
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive

/** The empty name is the unsectioned Bots group; stale names stay out and new groups follow. */
internal fun orderedSections(names: List<String>, saved: List<String>): List<String> =
    (saved.filter { it in names } + names).distinct()

internal fun renamedSectionOrder(order: List<String>, from: String, to: String?): List<String> =
    if (from == to) order else order.mapNotNull { if (it != from) it else to?.takeUnless(order::contains) }

internal object SectionOrders {
    fun key(account: String, computer: String): String = "sectionOrder:$account:$computer"
    fun load(context: Context, key: String): List<String> = try {
        Json.parseToJsonElement(rosterPrefs(context).getString(key, "[]") ?: "[]").jsonArray.map { it.jsonPrimitive.content }
    } catch (_: IllegalArgumentException) { emptyList() } // A malformed local preference starts with the default order.
    fun save(context: Context, key: String, names: List<String>) {
        rosterPrefs(context).edit { putString(key, JsonArray(names.map(::JsonPrimitive)).toString()) }
    }
}

/** A heading [reorder] drags by [name]: its height for the swaps, lifted above the rest while held, otherwise sliding into place. */
internal fun LazyItemScope.reorderable(reorder: ListReorder, name: String): Modifier = Modifier.onSizeChanged { reorder.heights[name] = it.height }
    .then(if (reorder.dragged == name) Modifier.zIndex(1f).graphicsLayer { translationY = reorder.translation(name) } else Modifier.animateItem())
