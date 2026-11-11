package com.codync.android

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import com.codync.android.design.AppLanguage
import com.codync.android.design.LocalAppLanguage
import com.codync.android.design.LocalPhrases
import com.codync.android.design.LocalSetAppLanguage
import com.codync.android.design.LocalSystemChinese
import com.codync.android.design.LocalSystemTag

/**
 * This device's choice of the app's own interface language, and the mirroring that comes with it.
 * Arabic turns the whole layout around, while each chat paragraph still reads in the direction of
 * its own text ([com.codync.android.design.byText]), so English inside an Arabic app stays left to right.
 * The choice is device-scoped and only affects the interface, never chats or bot names.
 */
@Composable
internal fun LanguageRoot(content: @Composable () -> Unit) {
    val context = LocalContext.current
    val configuration = LocalConfiguration.current
    val systemTag = if (configuration.locales.isEmpty) "en" else configuration.locales[0].toLanguageTag()
    var preference by remember { mutableStateOf(AppLanguageStore.get(context)) }
    val language = preference.resolved(systemTag)
    val phrases = remember(context, language) { AppLanguageStore.phrases(context, language, systemTag) }
    val systemChinese = remember(systemTag) { AppLanguage.System.resolved(systemTag).usesChinese(false) }
    CompositionLocalProvider(
        LocalAppLanguage provides preference,
        LocalSystemTag provides systemTag,
        LocalSystemChinese provides systemChinese,
        LocalPhrases provides phrases,
        LocalSetAppLanguage provides { chosen: AppLanguage ->
            preference = chosen
            AppLanguageStore.set(context, chosen)
        },
        LocalLayoutDirection provides if (language.readsRightToLeft(systemTag)) LayoutDirection.Rtl else LayoutDirection.Ltr,
        content = content,
    )
}
