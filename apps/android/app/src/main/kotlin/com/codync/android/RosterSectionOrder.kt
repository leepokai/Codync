package com.codync.android

import android.content.Context
import androidx.core.content.edit
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.codync.android.design.ContextDrag
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
    private fun prefs(context: Context) = context.getSharedPreferences("codync-roster", Context.MODE_PRIVATE)
    fun load(context: Context, key: String): List<String> = try {
        Json.parseToJsonElement(prefs(context).getString(key, "[]") ?: "[]").jsonArray.map { it.jsonPrimitive.content }
    } catch (_: IllegalArgumentException) { emptyList() } // A malformed local preference starts with the default order.
    fun save(context: Context, key: String, names: List<String>) {
        prefs(context).edit { putString(key, JsonArray(names.map(::JsonPrimitive)).toString()) }
    }
}

/** Touch and hold a heading, then drag it past neighbors. Only a completed drop is saved. */
internal class SectionReorder {
    var names: List<String> = emptyList()
    private var order by mutableStateOf<List<String>?>(null)
    val shown: List<String> get() = order ?: names
    var dragged by mutableStateOf<String?>(null)
        private set
    private var offset by mutableFloatStateOf(0f)
    private var origin = 0f
    val heights = mutableMapOf<String, Int>()
    /** LazyColumn item offsets, before graphics transforms, so folding and scrolling don't move the held header. */
    var position: (String) -> Float = { 0f }
    var save: (List<String>) -> Unit = {}
    private val handles = mutableMapOf<String, ContextDrag>()

    fun translation(name: String): Float = if (dragged == name) origin + offset - position(name) else 0f

    fun move(name: String, direction: Int) {
        val ids = shown
        val at = ids.indexOf(name)
        if (at < 0 || at + direction !in ids.indices) return
        val next = ids.toMutableList().apply { add(at + direction, removeAt(at)) }
        if (dragged == null) save(next) else order = next
    }

    private fun finish(commit: Boolean) {
        if (commit) order?.takeIf { it != names }?.let(save)
        dragged = null
        order = null
        offset = 0f
    }

    fun handle(name: String): ContextDrag = handles.getOrPut(name) {
        ContextDrag(start = { origin = position(name); order = names; dragged = name; offset = 0f }, move = { step ->
            offset += step
            val distance = translation(name)
            val direction = if (distance > 0) 1 else -1
            val next = shown.getOrNull(shown.indexOf(name) + direction)
            val height = next?.let(heights::get)
            if (height != null && kotlin.math.abs(distance) > height / 2f) move(name, direction)
        }, end = { finish(true) }, cancel = { finish(false) })
    }
}
