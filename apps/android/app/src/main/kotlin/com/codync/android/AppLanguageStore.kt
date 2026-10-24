package com.codync.android

import android.content.Context
import com.codync.android.design.AppLanguage
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Device preference and the canonical catalogs shared with the other clients. */
internal object AppLanguageStore {
    private const val KEY = "appLanguage"
    private val catalogs = mutableMapOf<AppLanguage, Map<String, String>>()
    private fun prefs(context: Context) = context.getSharedPreferences("codync-language", Context.MODE_PRIVATE)

    fun get(context: Context): AppLanguage = AppLanguage.fromStored(prefs(context).getString(KEY, null))

    fun set(context: Context, language: AppLanguage) {
        prefs(context).edit().putString(KEY, language.stored).apply()
        WidgetStore.refreshLanguage()
    }

    fun phrases(context: Context, language: AppLanguage = get(context), systemTag: String = java.util.Locale.getDefault().toLanguageTag()): Map<String, String> {
        val resolved = language.resolved(systemTag)
        if (resolved == AppLanguage.En) return emptyMap()
        synchronized(catalogs) {
            catalogs[resolved]?.let { return it }
            val name = if (resolved == AppLanguage.Zh) "zh-Hans" else resolved.stored
            return try {
                val text = context.assets.open("$name.json").bufferedReader().use { it.readText() }
                val parsed = Json.parseToJsonElement(text).jsonObject.mapValues { (_, value) -> value.jsonPrimitive.content }
                val table = if (resolved == AppLanguage.ZhHant) phrases(context, AppLanguage.Zh) + parsed else parsed
                catalogs[resolved] = table
                table
            } catch (error: Exception) {
                android.util.Log.e("Codync", "$name phrases failed to load", error)
                emptyMap()
            }
        }
    }
}
