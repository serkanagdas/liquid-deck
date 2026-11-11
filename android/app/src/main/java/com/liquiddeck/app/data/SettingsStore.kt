package com.liquiddeck.app.data

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.first

private val Context.deckDataStore by preferencesDataStore(name = "liquid_deck")

data class StoredSettings(
    val host: String = "",
    val token: String = "",
    val keepAwake: Boolean = true,
    val haptics: Boolean = true,
)

class SettingsStore(private val context: Context) {
    private object Keys {
        val host = stringPreferencesKey("host")
        val token = stringPreferencesKey("token")
        val keepAwake = booleanPreferencesKey("keep_awake")
        val haptics = booleanPreferencesKey("haptics")
    }

    suspend fun load(): StoredSettings {
        val prefs = context.deckDataStore.data.first()
        return StoredSettings(
            host = prefs[Keys.host].orEmpty(),
            token = prefs[Keys.token].orEmpty(),
            keepAwake = prefs[Keys.keepAwake] ?: true,
            haptics = prefs[Keys.haptics] ?: true,
        )
    }

    suspend fun saveHost(value: String) {
        context.deckDataStore.edit { it[Keys.host] = value }
    }

    suspend fun saveToken(value: String) {
        context.deckDataStore.edit { it[Keys.token] = value }
    }

    suspend fun saveKeepAwake(value: Boolean) {
        context.deckDataStore.edit { it[Keys.keepAwake] = value }
    }

    suspend fun saveHaptics(value: Boolean) {
        context.deckDataStore.edit { it[Keys.haptics] = value }
    }
}
