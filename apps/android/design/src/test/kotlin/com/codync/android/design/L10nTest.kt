package com.codync.android.design

import org.junit.Assert.assertEquals
import org.junit.Test

class L10nTest {
    private val phrases = mapOf(
        "connected" to "на связи",
        "Settings" to "Настройки",
        "Routine" to "Рутина",
        "Created bot {bot}" to "Создан бот {bot}",
        "Routine · {name}" to "Рутина · {name}",
        "Message from {bot}: {message}" to "Сообщение от {bot}: {message}",
        "Last run {status}" to "Последний запуск: {status}",
    )

    private fun ru(text: String) = translate(text, AppLanguage.Ru, false, phrases)

    @Test fun `bot, routine and message slots keep what the user wrote`() {
        assertEquals("Создан бот connected", ru("Created bot connected"))
        assertEquals("Рутина · Проверка почты", ru("Routine · Проверка почты"))
        assertEquals("Рутина · Settings", ru("Routine · Settings"))
        assertEquals("Сообщение от Reviewer: Settings", ru("Message from Reviewer: Settings"))
    }

    @Test fun `app-owned slots still translate`() {
        assertEquals("Последний запуск: на связи", ru("Last run connected"))
    }

    @Test fun `host schedule sentences match their template`() {
        val italian = mapOf("Every day at {times} · {zone}" to "Ogni giorno alle {times} · {zone}")
        assertEquals("Ogni giorno alle 09:00, 18:00 · Europe/Rome",
            translate("Every day at 09:00, 18:00 · Europe/Rome", AppLanguage.It, false, italian))
    }
}
