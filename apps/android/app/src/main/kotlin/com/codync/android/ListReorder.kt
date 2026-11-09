package com.codync.android

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.semantics.CustomAccessibilityAction
import com.codync.android.design.ContextDrag

/** Drags a row past its neighbors, or steps it a place with [move]. Only a completed drop is saved. */
internal class ListReorder {
    var names: List<String> = emptyList()
    private var order by mutableStateOf<List<String>?>(null)
    val shown: List<String> get() = order ?: names
    var dragged by mutableStateOf<String?>(null)
        private set
    private var offset by mutableFloatStateOf(0f)
    private var origin = 0f
    val heights = mutableMapOf<String, Int>()
    /** LazyColumn item offsets, before graphics transforms, so folding and scrolling don't move the held row. */
    var position: (String) -> Float = { 0f }
    var save: (List<String>) -> Unit = {}
    /** Just before a drag swaps two rows, while the old order is still laid out. */
    var beforeSwap: () -> Unit = {}
    private val handles = mutableMapOf<String, ContextDrag>()

    fun translation(name: String): Float = if (dragged == name) origin + offset - position(name) else 0f

    fun move(name: String, direction: Int) {
        val ids = shown
        val at = ids.indexOf(name)
        if (at < 0 || at + direction !in ids.indices) return
        val next = ids.toMutableList().apply { add(at + direction, removeAt(at)) }
        if (dragged == null) save(next) else { beforeSwap(); order = next }
    }

    private fun finish(commit: Boolean) {
        if (commit) order?.takeIf { it != names }?.let(save)
        dragged = null
        order = null
        offset = 0f
    }

    /** [name]'s row left the list: if it was the one held, its gesture will never end, so the drag ends here. */
    fun released(name: String) { if (dragged == name) finish(false) }

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

/** What TalkBack offers in place of a drag. */
internal fun moveActions(translate: (String) -> String, move: (Int) -> Unit): List<CustomAccessibilityAction> = listOf(
    CustomAccessibilityAction(translate("Move up")) { move(-1); true },
    CustomAccessibilityAction(translate("Move down")) { move(1); true },
)
