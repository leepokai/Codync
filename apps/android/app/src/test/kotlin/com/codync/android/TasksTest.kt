package com.codync.android

import com.codync.android.core.BotTask
import com.codync.android.core.Entry
import com.codync.android.core.MirrorEvent
import com.codync.android.core.TaskStatus
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

class TasksTest {
    private fun task(status: TaskStatus = TaskStatus.Working, activity: String = "", id: String = "task-1") =
        BotTask(id, "Fix login", status, activity, startedAt = 1)
    private val english = { text: String -> text }
    private val chinese = mapOf("Needs you" to "需要你", "Starting…" to "開始中…", "{count} tasks running" to "{count} 件事", "Thinking…" to "思考中…")
    private val traditional = { text: String -> chinese[text] ?: text }

    @Test fun `one task reads as its status, its step or its title`() {
        assertEquals("Needs you", tasksWhat(listOf(task(TaskStatus.NeedsInput, "Thinking…")), english))
        assertEquals("Starting…", tasksWhat(listOf(task(TaskStatus.Starting)), english))
        assertEquals("Thinking…", tasksWhat(listOf(task(activity = "Thinking…")), english))
        assertEquals("Fix login", tasksWhat(listOf(task()), english))
    }

    @Test fun `several tasks read as their count, and say when one needs the user`() {
        assertEquals("2 tasks running", tasksWhat(listOf(task(), task(TaskStatus.Starting, id = "task-2")), english))
        assertEquals("2 tasks running · needs you", tasksWhat(listOf(task(), task(TaskStatus.NeedsInput, id = "task-2")), english))
    }

    @Test fun `the words follow the app's language, the task's title stays as written`() {
        assertEquals("需要你", tasksWhat(listOf(task(TaskStatus.NeedsInput)), traditional))
        assertEquals("開始中…", tasksWhat(listOf(task(TaskStatus.Starting)), traditional))
        assertEquals("思考中…", tasksWhat(listOf(task(activity = "Thinking…")), traditional))
        assertEquals("Fix login", tasksWhat(listOf(task()), traditional))
        assertEquals("2 件事 · 需要你", tasksWhat(listOf(task(), task(TaskStatus.NeedsInput, id = "task-2")), traditional))
    }

    @Test fun `a task's entry never makes a chat unread`() {
        val entry = Entry("e", 1, "task-1", 1, "user", data = buildJsonObject { put("text", "hi") })
        assertEquals(emptyList<String>(), readAfter(listOf(MirrorEvent.TaskEntryChanged("task-1", entry)), listOf("task-1", "bot")))
    }

    @Test fun `the timer counts minutes, then hours`() {
        assertEquals("0:07", timerText(7_900))
        assertEquals("12:30", timerText(750_000))
        assertEquals("1:02:03", timerText(3_723_000))
        assertEquals("0:00", timerText(-5_000))
    }
}
