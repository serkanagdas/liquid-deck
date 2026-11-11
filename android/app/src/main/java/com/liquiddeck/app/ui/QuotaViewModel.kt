package com.liquiddeck.app.ui

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.liquiddeck.app.data.QuotaKind
import com.liquiddeck.app.data.QuotaProvider
import com.liquiddeck.app.data.QuotaRepository
import com.liquiddeck.app.data.QuotaSnapshot
import com.liquiddeck.app.data.QuotaState
import com.liquiddeck.app.data.QuotaStore
import java.util.UUID
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * Keeps the quota page fresh every three minutes, independent of the PC
 * connection.  Provider keys and the provider list live in DataStore.
 */
class QuotaViewModel(application: Application) : AndroidViewModel(application) {
    private val repo = QuotaRepository()
    private val store = QuotaStore(application)

    private val _state = MutableStateFlow(QuotaState())
    val state = _state.asStateFlow()

    init {
        viewModelScope.launch {
            val providers = store.load()
            _state.update { it.copy(providers = providers, loaded = true) }
            refreshAll(force = true)
            while (isActive) {
                delay(REFRESH_INTERVAL_MS)
                refreshAll(force = false)
            }
        }
    }

    fun refreshNow() {
        viewModelScope.launch { refreshAll(force = true) }
    }

    fun saveProvider(provider: QuotaProvider) {
        val clean = provider.copy(
            id = provider.id.ifBlank { UUID.randomUUID().toString() },
            label = provider.label.trim().ifBlank { QuotaKind.from(provider.kind).label },
            key = provider.key.trim(),
        )
        val next = _state.value.providers.toMutableList()
        val index = next.indexOfFirst { it.id == clean.id }
        if (index >= 0) next[index] = clean else next += clean
        _state.update { it.copy(providers = next) }
        viewModelScope.launch {
            store.save(next)
            refreshOne(clean, force = true)
        }
    }

    fun removeProvider(id: String) {
        val next = _state.value.providers.filterNot { it.id == id }
        _state.update {
            it.copy(providers = next, snapshots = it.snapshots - id, refreshing = it.refreshing - id)
        }
        viewModelScope.launch { store.save(next) }
    }

    private suspend fun refreshAll(force: Boolean) {
        val providers = _state.value.providers
        providers.forEach { provider -> refreshOne(provider, force) }
    }

    private suspend fun refreshOne(provider: QuotaProvider, force: Boolean) {
        val kind = QuotaKind.from(provider.kind)
        if (kind.needsKey && provider.key.isBlank()) {
            _state.update {
                it.copy(
                    snapshots = it.snapshots + (
                        provider.id to QuotaSnapshot(
                            providerId = provider.id,
                            error = "API anahtarı girilmedi.",
                        )
                        ),
                )
            }
            return
        }

        val previous = _state.value.snapshots[provider.id]
        val interval = if (kind == QuotaKind.CLAUDE) CLAUDE_INTERVAL_MS else REFRESH_INTERVAL_MS
        val age = System.currentTimeMillis() - (previous?.updatedAt ?: 0L)
        if (!force && previous != null && age in 0 until interval) return

        _state.update { it.copy(refreshing = it.refreshing + provider.id) }
        val fresh = repo.fetch(provider)
        _state.update { current ->
            val old = current.snapshots[provider.id]
            // Keep the last good numbers when a refresh fails, so a hiccup
            // does not blank out the card; only surface the error line.
            val merged = if (fresh.error != null && old != null && old.windows.isNotEmpty()) {
                old.copy(error = fresh.error)
            } else {
                fresh
            }
            current.copy(
                snapshots = current.snapshots + (provider.id to merged),
                refreshing = current.refreshing - provider.id,
            )
        }
    }

    private companion object {
        const val REFRESH_INTERVAL_MS = 180_000L
        const val CLAUDE_INTERVAL_MS = 300_000L
    }
}
