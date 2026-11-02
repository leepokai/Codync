package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.decodeFromJsonElement

@Serializable data class CustomIcon(val dataUrl: String, val tinted: Boolean = false)

fun decodeCustomIcons(value: JsonElement): Map<String, CustomIcon> = WireJson.decodeFromJsonElement(value)

/** Apply the current image snapshot, including removals, without replacing fresh bot metadata. */
fun List<Bot>.withCustomIcons(icons: Map<String, CustomIcon>): List<Bot> = map { it.copy(customIcon = icons["bot:${it.id}"]) }

/** A normalized image for each of the feed's 256 bots (every computer's), plus the 512 KiB summary allowance. */
const val MAX_WIDGET_FEED_BYTES = 256 * 65_558 + 512 * 1024
