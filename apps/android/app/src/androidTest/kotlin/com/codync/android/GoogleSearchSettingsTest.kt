package com.codync.android

import android.graphics.Bitmap
import android.os.Build
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

/** Actual settings on the paired disposable host, without using a provider key or microphone. */
class GoogleSearchSettingsTest {
    @get:Rule val compose = createComposeRule()

    @Test fun searchSwitchFollowsProviderAndModeAndKeepsTheChoice() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("googleSearchSettings") == "true")
        assumeTrue(Build.PRODUCT.startsWith("sdk"))
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val choices = VoicePreferences.choices(context)
        val oldProvider = choices.provider
        val oldMode = choices.speechMode(VoiceProvider.Gemini)
        val oldSearch = choices.googleSearch
        choices.provider = null; choices.speechMode(VoiceProvider.Gemini, false); choices.googleSearch = false
        var store: AppStore? = null
        try {
            compose.setContent {
                val model: AppStore = viewModel()
                store = model
                val state by model.state.collectAsState()
                Hosted { VoiceChatSettings(state, model) }
            }
            compose.runOnIdle { requireNotNull(store).setForeground(true) }
            compose.waitUntil(30_000) { store?.state?.value?.actualConnection is LinkState.Ready }
            assertTrue(runBlocking { requireNotNull(store).voiceStatus() }.googleSearch)
            compose.onNodeWithContentDescription("Engine:", substring = true).performClick()
            compose.onNodeWithText("Gemini", useUnmergedTree = true).performClick()
            val toggle = hasContentDescription("Google Search") and isToggleable()
            compose.waitUntil(10_000) { compose.onAllNodes(toggle).fetchSemanticsNodes().size == 1 }
            compose.onNode(toggle).performScrollTo().assertIsOff().performClick().assertIsOn()
            assertTrue(VoicePreferences.choices(context).googleSearch)
            compose.onRoot().captureToImage().asAndroidBitmap().let { image ->
                File(context.cacheDir, "google-search-settings.png").outputStream().use { image.compress(Bitmap.CompressFormat.PNG, 100, it) }
            }
            compose.onNodeWithContentDescription("Mode:", substring = true).performScrollTo().performClick()
            compose.onNodeWithText("Speech", useUnmergedTree = true).performClick()
            compose.onNode(toggle).assertDoesNotExist()
            compose.onNodeWithContentDescription("Mode:", substring = true).performClick()
            compose.onNodeWithText("Realtime", useUnmergedTree = true).performClick()
            compose.onNode(toggle).performScrollTo().assertIsOn()
            compose.onNodeWithContentDescription("Engine:", substring = true).performScrollTo().performClick()
            compose.onNodeWithText("OpenAI", useUnmergedTree = true).performClick()
            compose.onNode(toggle).assertDoesNotExist()
        } finally {
            compose.runOnIdle { store?.setForeground(false) }
            choices.provider = oldProvider; choices.speechMode(VoiceProvider.Gemini, oldMode); choices.googleSearch = oldSearch
        }
    }
}
