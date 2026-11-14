package com.codync.android

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale

/** The iOS app's date stamps (CodyncKit `RelativeTime`), in the phone's locale. */
internal object RelativeTime {
    private fun date(milliseconds: Long) = Instant.ofEpochMilli(milliseconds).atZone(ZoneId.systemDefault())
    private val time get() = DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(Locale.getDefault())
    private val monthDay get() = DateTimeFormatter.ofPattern(android.text.format.DateFormat.getBestDateTimePattern(Locale.getDefault(), "MMMd"))

    /** Grok-style compact age: now · 5m · 3h · 2d · Sep 3. */
    fun short(milliseconds: Long, now: Long = System.currentTimeMillis()): String {
        val s = maxOf(0L, (now - milliseconds) / 1000)
        return when {
            s < 60 -> "now"
            s < 3600 -> "${s / 60}m"
            s < 86_400 -> "${s / 3600}h"
            s < 86_400 * 7 -> "${s / 86_400}d"
            else -> date(milliseconds).format(monthDay)
        }
    }

    /** Roster stamp: 3:45 AM · Yesterday · Wednesday · Sep 16. */
    fun day(milliseconds: Long, now: Long = System.currentTimeMillis(), yesterday: String = "Yesterday"): String {
        val day = date(milliseconds)
        val today = LocalDate.now(ZoneId.systemDefault())
        return when {
            day.toLocalDate() == today -> day.format(time)
            day.toLocalDate() == today.minusDays(1) -> yesterday
            now - milliseconds < 6 * 86_400_000L -> day.format(DateTimeFormatter.ofPattern("EEEE", Locale.getDefault()))
            else -> day.format(monthDay)
        }
    }

    /** When something ahead happens: 3:00 PM today, Tue 3:00 PM another day. */
    fun clock(milliseconds: Long): String {
        val day = date(milliseconds)
        val at = day.format(time)
        return if (day.toLocalDate() == LocalDate.now(ZoneId.systemDefault())) at
        else day.format(DateTimeFormatter.ofPattern("EEE", Locale.getDefault())) + " " + at
    }

    /** A calendar date, as iOS's abbreviated one: Oct 20, 2026. */
    fun calendarDay(milliseconds: Long): String =
        date(milliseconds).format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM).withLocale(Locale.getDefault()))

    /** Chat separator: Today 3:27 AM · Yesterday 5:20 PM · Sep 16 9:02 AM. */
    fun separator(milliseconds: Long, todayLabel: String = "Today", yesterdayLabel: String = "Yesterday"): String {
        val day = date(milliseconds)
        val today = LocalDate.now(ZoneId.systemDefault())
        val name = when (day.toLocalDate()) {
            today -> todayLabel
            today.minusDays(1) -> yesterdayLabel
            else -> day.format(monthDay)
        }
        return name + " " + day.format(time)
    }
}
