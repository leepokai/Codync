package com.codync.android.core

/**
 * Options of the visual routine schedule editor, mirroring the styles the host's schedule editor
 * understands (`host/src/routines/editor.rs`). The host builds the cron expression, validates it and
 * computes the next runs; this only lists what the editor can offer.
 */
object RoutineSchedule {
    /** Trigger kinds the editor edits; any other (`once`, several triggers) keeps the original triggers. */
    val editableKinds = listOf("cron", "interval", "event", "webhook")
    /** Style → title, in display order. `custom` is the raw cron expression. */
    val styles = listOf(
        "minutes" to "Every few minutes", "hourly" to "Hourly", "daily" to "Daily", "weekdays" to "Weekdays",
        "weekly" to "Weekly", "days" to "Chosen days", "monthly" to "Monthly", "yearly" to "Yearly", "custom" to "Cron",
    )
    /** Minute steps that divide the hour evenly; the host rejects others. */
    val minuteSteps = listOf(1, 5, 10, 15, 20, 30)
    /** The steps an hourly run repeats at, the choices the host accepts. */
    val everyHours = listOf(1 to "hour", 2 to "2 hours", 3 to "3 hours", 4 to "4 hours", 6 to "6 hours", 8 to "8 hours", 12 to "12 hours")
    /** Interval unit in seconds → title; the host accepts exactly these. */
    val intervalUnits = listOf(1L to "Seconds", 60L to "Minutes", 3600L to "Hours", 86400L to "Days")
    /** Weekdays as the host numbers them: 0 Sunday … 6 Saturday. */
    val weekdayInitials = listOf("S", "M", "T", "W", "T", "F", "S")
    val weekdayNames = listOf("Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday")
    val monthNames = listOf("January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December")

    /** Styles whose cron has an hour as well as a minute. */
    fun usesTime(style: String): Boolean = style in listOf("daily", "weekdays", "weekly", "days", "monthly", "yearly")

    /** The days a month has, counting the leap year, so February 29 is a valid yearly date. */
    fun daysInMonth(month: Int): Int = when (month) {
        1, 3, 5, 7, 8, 10, 12 -> 31
        4, 6, 9, 11 -> 30
        2 -> 29
        else -> 0
    }
}
