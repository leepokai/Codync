package com.codync.android.core

import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull

/** The host retains internal work for ordering and agent context, while clients render only its public entries. */
val Entry.isInternal: Boolean get() = (data["internal"] as? JsonPrimitive)?.booleanOrNull == true ||
    kind == "user" && data.text("from") == "proactive"

/** Also applied to restored mirrors: cached internal entries never become visible after reconnecting. */
fun List<Entry>.visibleEntries(): List<Entry> = filterNot(Entry::isInternal)
