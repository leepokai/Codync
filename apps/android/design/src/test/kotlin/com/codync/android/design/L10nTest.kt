package com.codync.android.design

import org.junit.Assert.assertEquals
import org.junit.Test

class L10nTest {
    private val phrases = mapOf(
        "connected" to "на связи",
        "Settings" to "Настройки",
        "Routine" to "Рутина",
        "this Mac" to "этот Mac",
        "Created bot {bot}" to "Создан бот {bot}",
        "Routine · {routine}" to "Рутина · {routine}",
        "Message from {bot}: {message}" to "Сообщение от {bot}: {message}",
        "Runs the official installer on {name}." to "Запускает официальный установщик на {name}.",
    )

    private fun ru(text: String) = translate(text, AppLanguage.Ru, false, phrases)

    @Test fun `bot, routine and message slots keep what the user wrote`() {
        assertEquals("Создан бот connected", ru("Created bot connected"))
        assertEquals("Рутина · Проверка почты", ru("Routine · Проверка почты"))
        assertEquals("Рутина · Settings", ru("Routine · Settings"))
        assertEquals("Сообщение от Reviewer: Settings", ru("Message from Reviewer: Settings"))
    }

    @Test fun `other slots still translate`() {
        assertEquals("Запускает официальный установщик на этот Mac.", ru("Runs the official installer on this Mac."))
    }
}
