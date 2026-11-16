package com.codync.android.core

private val PLACEHOLDER = Regex("""\{(\w+)\}""")

/** Fills a template's `{key}`s in one pass: a device named "{code}" stays as written. */
fun fill(template: String, values: Map<String, String>): String =
    PLACEHOLDER.replace(template) { values[it.groupValues[1]] ?: it.value }

fun fill(template: String, vararg values: Pair<String, String>): String = fill(template, values.toMap())
