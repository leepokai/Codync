package com.codync.android.core

import kotlinx.serialization.Serializable

/** A bot's character in three layers the user writes (the host's `agent::personality`). */
@Serializable data class Personality(val soul: String = "", val persona: String = "", val heartbeat: String = "")

enum class PersonalityLayer {
    Soul, Persona, Heartbeat;

    fun of(personality: Personality): String = when (this) {
        Soul -> personality.soul
        Persona -> personality.persona
        Heartbeat -> personality.heartbeat
    }

    fun set(personality: Personality, text: String): Personality = when (this) {
        Soul -> personality.copy(soul = text)
        Persona -> personality.copy(persona = text)
        Heartbeat -> personality.copy(heartbeat = text)
    }
}
