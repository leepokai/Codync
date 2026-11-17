package com.codync.android

import com.codync.android.core.Resting
import com.codync.android.design.AppLanguage
import com.codync.android.design.translate
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertNotEquals
import org.junit.Test
import java.io.File

class FallbackLinesTest {
    private val zhHant = Json.parseToJsonElement(File("../../ios/Kit/Sources/CodyncKit/Resources/Languages/zh-Hant.json").readText())
        .jsonObject.mapValues { it.value.jsonPrimitive.content }

    @Test fun `a rest line reads in the app's language`() {
        // The filled line goes through the catalog's "{time}" templates: a reworded line or key falls back to English.
        for (reason in listOf("usageLimit", "signedOut")) {
            val line = Resting(agent = "claude", reason = reason).line("3:00 PM")
            assertNotEquals(reason, line, translate(line, AppLanguage.ZhHant, false, zhHant))
        }
    }
}
