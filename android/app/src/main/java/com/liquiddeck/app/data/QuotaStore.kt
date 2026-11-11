package com.liquiddeck.app.data

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.first
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

private val Context.quotaDataStore by preferencesDataStore(name = "liquid_deck_quota")

private val quotaJson = Json {
    ignoreUnknownKeys = true
    encodeDefaults = true
}

/**
 * Persists the provider list (including API keys) on this device only.  The
 * keys never leave the phone except in the requests to their own vendor.
 */
class QuotaStore(private val context: Context) {
    private val providersKey = stringPreferencesKey("providers")

    suspend fun load(): List<QuotaProvider> {
        val raw = context.quotaDataStore.data.first()[providersKey] ?: return emptyList()
        return runCatching {
            quotaJson.decodeFromString<List<QuotaProvider>>(raw)
        }.getOrDefault(emptyList())
    }

    suspend fun save(providers: List<QuotaProvider>) {
        context.quotaDataStore.edit { prefs ->
            prefs[providersKey] = quotaJson.encodeToString(providers)
        }
    }
}
