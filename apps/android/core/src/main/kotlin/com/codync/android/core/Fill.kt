package com.codync.android.core

private val PLACEHOLDER = Regex("""\{(\w+)\}""")

/** Fills a template's `{key}`s in one pass: a device named "{code}" stays as written. */
fun fill(template: String, vararg values: Pair<String, String>): String {
    val named = values.toMap()
    return PLACEHOLDER.replace(template) { named[it.groupValues[1]] ?: it.value }
}
