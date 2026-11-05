package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.codync.android.core.RoutineSchedule
import com.codync.android.core.RoutineScheduleDraft
import com.codync.android.core.RoutineSchedulePreview
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import java.text.DateFormat
import java.util.Date

/**
 * The visual fields of a recurring routine schedule (days, times, intervals, zone). The host turns them into
 * a cron expression, validates them and reports the next run; nothing here computes a date.
 */
@Composable internal fun ScheduleFields(schedule: RoutineScheduleDraft, change: (RoutineScheduleDraft) -> Unit, preview: RoutineSchedulePreview?, error: String?) {
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Field("Repeats") {
            ChoicePicker(schedule.calendarStyle, RoutineSchedule.styles, { change(schedule.copy(calendarStyle = it)) }, label = "Repeats")
        }
        when {
            schedule.calendarStyle == "custom" -> Field("Cron") {
                FieldBox(schedule.expression, { change(schedule.copy(expression = it)) }, "0 9 * * 1-5", style = Type.body.monospaced,
                    fill = Palette.bubbleUser, capitalize = false)
            }
            schedule.calendarStyle == "minutes" -> Field("Every") {
                ChoicePicker(schedule.minuteStep, RoutineSchedule.minuteSteps.map { it to "$it min" }, { change(schedule.copy(minuteStep = it)) }, label = "Minutes")
            }
            schedule.calendarStyle == "weekly" -> Field("Day") {
                ChoicePicker(schedule.weekday, RoutineSchedule.weekdayNames.withIndex().map { it.index to it.value }, { change(schedule.copy(weekday = it)) }, label = "Day")
            }
            schedule.calendarStyle == "days" -> Field("Days") { WeekdayChips(schedule.selectedDays) { change(schedule.toggleDay(it)) } }
            schedule.calendarStyle == "monthly" -> Field("Day of the month") {
                ChoicePicker(schedule.monthDay, (1..31).map { it to it.toString() }, { change(schedule.copy(monthDay = it)) }, label = "Day of the month")
            }
            schedule.calendarStyle == "yearly" -> Field("On") {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    ChoicePicker(schedule.yearMonth, RoutineSchedule.monthNames.withIndex().map { it.index + 1 to it.value },
                        { change(schedule.copy(yearMonth = it, monthDay = minOf(schedule.monthDay, RoutineSchedule.daysInMonth(it)))) }, label = "Month")
                    ChoicePicker(schedule.monthDay, (1..RoutineSchedule.daysInMonth(schedule.yearMonth)).map { it to it.toString() },
                        { change(schedule.copy(monthDay = it)) }, label = "Day")
                }
            }
        }
        when {
            schedule.calendarStyle == "hourly" && schedule.windowStart == null && schedule.windowEnd == null -> {
                Field("Minute past the hour") { MinutePicker(schedule.minute) { change(schedule.copy(minute = it)) } }
                Field("Every") {
                    ChoicePicker(schedule.everyHours, RoutineSchedule.everyHours, { change(schedule.copy(everyHours = it)) }, label = "Every")
                }
            }
            schedule.calendarStyle == "hourly" -> {
                // A window's start sets the minute of every run in it.
                Field("Window start") { TimePickers(schedule.windowStart ?: 1439) { change(schedule.copy(windowStart = it)) } }
                Field("Window end") { TimePickers(schedule.windowEnd ?: 1439) { change(schedule.copy(windowEnd = it)) } }
                Row(Modifier.padding(start = 4.dp)) {
                    LinkButton("Run through the whole day") { change(schedule.copy(windowStart = null, windowEnd = null)) }
                }
                Field("Every") {
                    ChoicePicker(schedule.everyHours, RoutineSchedule.everyHours, { change(schedule.copy(everyHours = it)) }, label = "Every")
                }
            }
            RoutineSchedule.usesTime(schedule.calendarStyle) -> Field("At") {
                TimePickers(schedule.hour * 60 + schedule.minute) { change(schedule.copy(hour = it / 60, minute = it % 60)) }
            }
        }
        Field("Time zone", "The schedule follows this zone, including daylight saving.") {
            FieldBox(schedule.zone, { change(schedule.copy(zone = it)) }, "Asia/Taipei", fill = Palette.bubbleUser, capitalize = false)
        }
        SchedulePreview(preview, error)
    }
}

/** Every N units, for the `interval` kind. */
@Composable internal fun IntervalFields(schedule: RoutineScheduleDraft, change: (RoutineScheduleDraft) -> Unit, preview: RoutineSchedulePreview?, error: String?) {
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Field("Every") {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { FieldBox(schedule.amount, { change(schedule.copy(amount = it.filter(Char::isDigit).take(6))) }, "1", fill = Palette.bubbleUser, capitalize = false) }
                ChoicePicker(schedule.unit, RoutineSchedule.intervalUnits, { change(schedule.copy(unit = it)) }, label = "Unit")
            }
        }
        SchedulePreview(preview, error)
    }
}

@Composable private fun MinutePicker(minute: Int, change: (Int) -> Unit) =
    ChoicePicker(minute, (0..59).map { it to "%02d".format(it) }, change, label = "Minute")

/** A time of day as its minutes since midnight, hour and minute past midnight. */
@Composable private fun TimePickers(minutes: Int, change: (Int) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        ChoicePicker(minutes / 60, (0..23).map { it to "%02d".format(it) }, { change(it * 60 + minutes % 60) }, label = "Hour")
        Text(":", style = Type.body, color = Palette.secondary)
        MinutePicker(minutes % 60) { change(minutes / 60 * 60 + it) }
    }
}

/** Seven round toggles, Sunday first as the host numbers them; the host rejects an empty choice. */
@Composable private fun WeekdayChips(selected: List<Int>, toggle: (Int) -> Unit) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        for (day in 0..6) {
            val on = day in selected
            Box(Modifier.size(38.dp).clip(Capsule()).background(if (on) Palette.accentFill else Palette.bubbleUser)
                .semantics { contentDescription = RoutineSchedule.weekdayNames[day] }
                .toggleable(value = on, role = Role.Checkbox, onValueChange = { toggle(day) }), contentAlignment = Alignment.Center) {
                Text(RoutineSchedule.weekdayInitials[day], style = Type.caption.medium, color = if (on) Palette.onAccent else Palette.text)
            }
        }
    }
}

/** One quiet line under the fields: what the host read, when it runs next, or why it can't. */
@Composable internal fun SchedulePreview(preview: RoutineSchedulePreview?, error: String?) {
    Column(Modifier.padding(start = 4.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        when {
            error != null -> Text(error, style = Type.caption, color = Palette.danger)
            preview != null -> {
                Text(preview.summary, style = Type.caption.medium, color = Palette.text)
                preview.nextRunAt?.let { Text("Next ${DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(it))}", style = Type.caption, color = Palette.secondary) }
                preview.warning?.let { Text(it, style = Type.caption, color = Palette.secondary) }
            }
            else -> Text("Checking this schedule…", style = Type.caption, color = Palette.tertiary)
        }
    }
}
