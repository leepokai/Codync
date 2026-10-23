package com.codync.android

import android.content.Context
import com.codync.android.design.AppLanguage
import com.codync.android.design.ChineseScript
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** This phone's interface language. The phrase tables are the same files the iPhone app ships. */
internal object AppLanguageStore {
    private const val KEY = "appLanguage"
    private val phraseLock = Any()

    @Volatile private var hans: Map<String, String>? = null
    @Volatile private var hant: Map<String, String>? = null
    @Volatile private var hantView: Map<String, String>? = null

    private fun prefs(context: Context) = context.getSharedPreferences("codync-language", Context.MODE_PRIVATE)

    fun get(context: Context): AppLanguage = AppLanguage.fromStored(prefs(context).getString(KEY, null))

    fun set(context: Context, language: AppLanguage) {
        prefs(context).edit().putString(KEY, AppLanguage.stored(language)).apply()
    }

    /** The table for this choice. A Traditional line missing from its table keeps the Simplified one. */
    fun phrases(context: Context, language: AppLanguage = get(context), systemTag: String = java.util.Locale.getDefault().toLanguageTag()): Map<String, String> {
        val simplified = load(context, ChineseScript.Hans)
        if (language.resolvedScript(systemTag) != ChineseScript.Hant) return simplified
        hantView?.let { return it }
        synchronized(phraseLock) {
            hantView?.let { return it }
            val merged = simplified + load(context, ChineseScript.Hant)
            hantView = merged
            return merged
        }
    }

    /** A missing or malformed table is logged and left empty so the next call can try again. */
    private fun load(context: Context, script: ChineseScript): Map<String, String> {
        val cached = if (script == ChineseScript.Hant) hant else hans
        cached?.let { return it }
        synchronized(phraseLock) {
            val again = if (script == ChineseScript.Hant) hant else hans
            again?.let { return it }
            val name = if (script == ChineseScript.Hant) "zh-Hant.json" else "zh-Hans.json"
            return try {
                val text = context.assets.open(name).bufferedReader().use { it.readText() }
                val parsed = Json.parseToJsonElement(text).jsonObject.mapValues { (_, value) -> value.jsonPrimitive.content }
                if (script == ChineseScript.Hant) hant = parsed else hans = parsed
                parsed
            } catch (error: Exception) {
                android.util.Log.e("Codync", "$name phrases failed to load", error)
                emptyMap()
            }
        }
    }
}
