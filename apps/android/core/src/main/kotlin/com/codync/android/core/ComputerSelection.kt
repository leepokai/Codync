package com.codync.android.core

/**
 * The computers the roster shows, from the ones left out ([hidden], comma-separated, kept on the device): an empty list
 * means all of them, newly paired ones included, and at least one always shows (iOS `ComputerSelection`).
 */
class ComputerSelection(val all: List<String>, hidden: String) {
    private val hidden = hidden.split(',').filter(String::isNotEmpty).toSet()

    val shown: List<String> get() = all.filter { it !in hidden }.ifEmpty { all }

    /** [id] shown or left out, keeping one shown. */
    fun toggling(id: String): String {
        if (id !in all) return encoded(all.toSet() - shown.toSet())
        val visible = shown.toMutableSet()
        if (id in visible) { if (visible.size > 1) visible.remove(id) } else visible.add(id)
        return encoded(all.toSet() - visible)
    }

    fun only(id: String): String = if (id !in all) "" else encoded(all.toSet() - id)

    private fun encoded(ids: Set<String>): String = ids.sorted().joinToString(",")
}
