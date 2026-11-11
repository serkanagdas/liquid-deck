package com.liquiddeck.app.ui

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.liquiddeck.app.data.ActionResult
import com.liquiddeck.app.data.Board
import com.liquiddeck.app.data.Connection
import com.liquiddeck.app.data.DeckButton
import com.liquiddeck.app.data.DeckConfig
import com.liquiddeck.app.data.DeckRepository
import com.liquiddeck.app.data.DeckState
import com.liquiddeck.app.data.MediaSnapshot
import com.liquiddeck.app.data.Metrics
import com.liquiddeck.app.data.ServerEvent
import com.liquiddeck.app.data.SettingsStore
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

data class UiState(
    val host: String = "",
    val token: String = "",
    val keepAwake: Boolean = true,
    val haptics: Boolean = true,
    val configured: Boolean = false,
    val connecting: Boolean = false,
    val connection: Connection = Connection.Offline(),
    val metrics: Metrics? = null,
    val config: DeckConfig? = null,
    val deckState: DeckState = DeckState(),
    val activeBoardId: String = "",
    val serverVersion: String = "",
    val showSettings: Boolean = false,
    val editMode: Boolean = false,
    val editor: KeyEditor? = null,
    val error: String? = null,
    val busyButtons: Set<String> = emptySet(),
) {
    val boards: List<Board> get() = config?.boards.orEmpty()

    val activeBoard: Board?
        get() = boards.firstOrNull { it.id == activeBoardId } ?: boards.firstOrNull()

    val media: MediaSnapshot? get() = metrics?.media

    fun isActive(boardId: String, buttonId: String): Boolean =
        deckState.toggles["$boardId/$buttonId"] == true
}

/** Draft state for the on-phone key editor. `buttonId == null` adds a new key. */
data class KeyEditor(
    val boardId: String,
    val buttonId: String?,
    val draft: DeckButton,
    val saving: Boolean = false,
    val error: String? = null,
) {
    val isNew: Boolean get() = buttonId == null
}

class DeckViewModel(application: Application) : AndroidViewModel(application) {
    private val repo = DeckRepository()
    private val store = SettingsStore(application)

    private val _ui = MutableStateFlow(UiState())
    val ui = _ui.asStateFlow()

    private val _messages = MutableSharedFlow<String>(
        extraBufferCapacity = 8,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    val messages = _messages.asSharedFlow()

    init {
        viewModelScope.launch {
            repo.events.collect(::onEvent)
        }
        viewModelScope.launch {
            repo.connection.collect { connection ->
                _ui.update { it.copy(connection = connection) }
            }
        }
        viewModelScope.launch {
            val saved = store.load()
            _ui.update {
                it.copy(
                    host = saved.host,
                    token = saved.token,
                    keepAwake = saved.keepAwake,
                    haptics = saved.haptics,
                    configured = saved.host.isNotBlank(),
                )
            }
            if (saved.host.isNotBlank()) {
                repo.start(saved.host, saved.token)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Connection
    // -----------------------------------------------------------------------

    /** Called from the connect screen: verifies the server before opening. */
    fun connect(host: String, token: String) {
        val trimmed = host.trim()
        if (trimmed.isBlank()) {
            _ui.update { it.copy(error = "Sunucu adresi gerekli.") }
            return
        }
        viewModelScope.launch {
            _ui.update { it.copy(connecting = true, error = null) }
            val probe = DeckRepository.probe(trimmed, token)
            if (!probe.ok) {
                _ui.update { it.copy(connecting = false, error = probe.message) }
                return@launch
            }
            store.saveHost(trimmed)
            store.saveToken(token)
            _ui.update {
                it.copy(
                    host = trimmed,
                    token = token,
                    configured = true,
                    connecting = false,
                    error = null,
                    metrics = null,
                    config = null,
                    deckState = DeckState(),
                    activeBoardId = "",
                    serverVersion = probe.version.orEmpty(),
                )
            }
            repo.start(trimmed, token)
        }
    }

    fun reconnect() {
        val current = _ui.value
        if (current.host.isBlank()) return
        repo.start(current.host, current.token)
        toast("Yeniden bağlanılıyor…")
    }

    fun disconnect() {
        repo.stop()
        _ui.update {
            it.copy(
                configured = false,
                metrics = null,
                config = null,
                deckState = DeckState(),
                activeBoardId = "",
                showSettings = false,
                connection = Connection.Offline(),
            )
        }
    }

    fun applySettings(host: String, token: String, keepAwake: Boolean, haptics: Boolean) {
        viewModelScope.launch {
            store.saveKeepAwake(keepAwake)
            store.saveHaptics(haptics)
        }
        val trimmed = host.trim()
        val changed = trimmed != _ui.value.host || token != _ui.value.token
        _ui.update {
            it.copy(
                keepAwake = keepAwake,
                haptics = haptics,
                showSettings = false,
            )
        }
        if (trimmed.isBlank()) {
            disconnect()
            return
        }
        if (changed) {
            viewModelScope.launch {
                store.saveHost(trimmed)
                store.saveToken(token)
                _ui.update {
                    it.copy(
                        host = trimmed,
                        token = token,
                        configured = true,
                        metrics = null,
                        config = null,
                        deckState = DeckState(),
                        activeBoardId = "",
                    )
                }
                repo.start(trimmed, token)
            }
        }
    }

    fun setShowSettings(show: Boolean) {
        _ui.update { it.copy(showSettings = show) }
    }

    // -----------------------------------------------------------------------
    // Server events
    // -----------------------------------------------------------------------

    private fun onEvent(event: ServerEvent) {
        when (event) {
            is ServerEvent.Hello -> {
                val config = event.config
                _ui.update { current ->
                    current.copy(
                        metrics = event.metrics ?: current.metrics,
                        config = config ?: current.config,
                        deckState = event.state ?: current.deckState,
                        activeBoardId = boardIdFor(
                            config = config ?: current.config,
                            preferred = event.state?.activeBoard,
                            fallback = current.activeBoardId,
                        ),
                        serverVersion = event.version.ifBlank { current.serverVersion },
                        error = null,
                    )
                }
            }

            is ServerEvent.Sample -> {
                _ui.update { it.copy(metrics = event.metrics, error = null) }
            }

            is ServerEvent.ConfigUpdate -> {
                _ui.update { current ->
                    current.copy(
                        config = event.config,
                        deckState = event.state,
                        activeBoardId = boardIdFor(
                            config = event.config,
                            preferred = event.state.activeBoard,
                            fallback = current.activeBoardId,
                        ),
                    )
                }
            }

            is ServerEvent.StateUpdate -> {
                _ui.update { current ->
                    val active = event.state.activeBoard
                        .takeIf { id -> current.boards.any { it.id == id } }
                        ?: current.activeBoardId
                    current.copy(deckState = event.state, activeBoardId = active)
                }
            }

            is ServerEvent.ActionResult, ServerEvent.Unknown -> Unit
        }
    }

    private fun boardIdFor(config: DeckConfig?, preferred: String?, fallback: String?): String {
        val boards = config?.boards.orEmpty()
        return preferred?.takeIf { id -> boards.any { it.id == id } }
            ?: fallback?.takeIf { id -> boards.any { it.id == id } }
            ?: config?.settings?.defaultBoard?.takeIf { id -> boards.any { it.id == id } }
            ?: boards.firstOrNull()?.id.orEmpty()
    }

    // -----------------------------------------------------------------------
    // Deck
    // -----------------------------------------------------------------------

    fun selectBoard(boardId: String) {
        if (_ui.value.boards.none { it.id == boardId }) return
        if (_ui.value.activeBoardId == boardId) return
        _ui.update { it.copy(activeBoardId = boardId) }
        viewModelScope.launch { repo.postActiveBoard(boardId) }
    }

    fun press(board: Board, button: DeckButton) {
        when (button.action) {
            "board" -> {
                val target = _ui.value.boards.firstOrNull { it.id == button.value }
                if (target == null) toast("Hedef pano bulunamadı.")
                else selectBoard(target.id)
                return
            }

            "panel" -> {
                when (button.value) {
                    "settings" -> _ui.update { it.copy(showSettings = true) }
                    "audio" -> toast("Ses mikseri PC'deki web panelinde.")
                    else -> toast("Düzenleyici PC'deki web panelinde.")
                }
                return
            }

            "none" -> {
                toast("Bu tuş henüz yapılandırılmadı.")
                return
            }
        }

        val active = _ui.value.isActive(board.id, button.id)
        val payload = payloadFor(button, active) ?: run {
            toast("Desteklenmeyen eylem: ${button.action}")
            return
        }

        val key = "${board.id}/${button.id}"
        if (key in _ui.value.busyButtons) return
        _ui.update { it.copy(busyButtons = it.busyButtons + key) }

        viewModelScope.launch {
            try {
                val result = repo.action(payload)
                if (result.ok) {
                    if (toggles(button)) {
                        val next = !active
                        setLocalToggle(board.id, button.id, next)
                        repo.postState(board.id, button.id, next)
                    }
                    val message = result.message
                    if (message.isNotBlank() && message != "ok") toast(message)
                } else {
                    toast(result.message.ifBlank { "Eylem başarısız." })
                }
            } finally {
                _ui.update { it.copy(busyButtons = it.busyButtons - key) }
            }
        }
    }

    private fun setLocalToggle(boardId: String, buttonId: String, active: Boolean) {
        _ui.update { current ->
            val toggles = current.deckState.toggles.toMutableMap()
            toggles["$boardId/$buttonId"] = active
            current.copy(
                deckState = current.deckState.copy(
                    toggles = toggles,
                    updatedAt = System.currentTimeMillis() / 1000,
                ),
            )
        }
    }

    // -----------------------------------------------------------------------
    // Deck editor — edit keys from the phone and save the whole config back
    // -----------------------------------------------------------------------

    fun toggleEditMode() {
        _ui.update { it.copy(editMode = !it.editMode) }
    }

    fun editKey(boardId: String, button: DeckButton?) {
        val board = _ui.value.boards.firstOrNull { it.id == boardId } ?: return
        val draft = button ?: DeckButton(
            id = "",
            label = "Yeni tuş",
            subtitle = "",
            icon = "bolt",
            color = "cyan",
            action = "none",
        )
        _ui.update { it.copy(editor = KeyEditor(board.id, button?.id, draft)) }
    }

    fun updateDraft(transform: (DeckButton) -> DeckButton) {
        _ui.update { state ->
            val editor = state.editor ?: return@update state
            state.copy(editor = editor.copy(draft = transform(editor.draft), error = null))
        }
    }

    fun closeEditor() {
        _ui.update { it.copy(editor = null) }
    }

    fun saveEditor() {
        val state = _ui.value
        val editor = state.editor ?: return
        val config = state.config ?: return
        if (editor.draft.label.isBlank()) {
            toast("Tuşa bir ad ver.")
            return
        }
        val board = config.boards.firstOrNull { it.id == editor.boardId } ?: return
        val buttons = if (editor.isNew) {
            board.buttons + editor.draft.copy(id = "")
        } else {
            board.buttons.map { if (it.id == editor.buttonId) editor.draft else it }
        }
        persist(
            config.copy(
                boards = config.boards.map {
                    if (it.id == board.id) it.copy(buttons = buttons) else it
                },
            ),
            editor,
            deleting = false,
        )
    }

    fun deleteEditedKey() {
        val state = _ui.value
        val editor = state.editor ?: return
        val config = state.config ?: return
        if (editor.isNew) {
            closeEditor()
            return
        }
        persist(
            config.copy(
                boards = config.boards.map { board ->
                    if (board.id == editor.boardId) {
                        board.copy(buttons = board.buttons.filterNot { it.id == editor.buttonId })
                    } else {
                        board
                    }
                },
            ),
            editor,
            deleting = true,
        )
    }

    private fun persist(config: DeckConfig, editor: KeyEditor, deleting: Boolean) {
        _ui.update { it.copy(editor = it.editor?.copy(saving = true, error = null)) }
        viewModelScope.launch {
            val result = repo.putConfig(config)
            if (!result.ok) {
                val message = result.message.ifBlank { "Pano kaydedilemedi." }
                _ui.update { it.copy(editor = it.editor?.copy(saving = false, error = message)) }
                toast(message)
                return@launch
            }
            val fresh = result.config ?: config
            _ui.update { current ->
                current.copy(
                    editor = null,
                    config = fresh,
                    activeBoardId = boardIdFor(fresh, current.activeBoardId, editor.boardId),
                )
            }
            toast(
                when {
                    deleting -> "Tuş silindi"
                    editor.isNew -> "Tuş eklendi"
                    else -> "Tuş güncellendi"
                },
            )
        }
    }

    private fun payloadFor(button: DeckButton, active: Boolean): JsonObject? {
        val amount = button.amount.coerceIn(1, 100)
        return when (button.action) {
            "discord_mute" -> buildJsonObject { put("action", "discord_mute") }
            "discord_deafen" -> buildJsonObject { put("action", "discord_deafen") }
            "media" -> buildJsonObject {
                put("action", "media")
                put("command", button.value.ifBlank { "play_pause" })
            }
            "volume" -> buildJsonObject {
                put("action", "volume")
                put("direction", if (button.value == "down") "down" else "up")
                put("step", amount)
            }
            "mute" -> buildJsonObject { put("action", "mute") }
            "key" -> buildJsonObject {
                put("action", "key")
                put("combo", button.value)
            }
            "script" -> buildJsonObject {
                put("action", "script")
                put("name", button.value)
            }
            "launch" -> buildJsonObject {
                put("action", "launch")
                put("target", button.value.ifBlank { "terminal" })
            }
            "terminal" -> buildJsonObject { put("action", "terminal") }
            "game_mode" -> buildJsonObject {
                put("action", "game_mode")
                put("enabled", !active)
            }
            "url" -> buildJsonObject {
                put("action", "url")
                put("url", button.value)
            }
            "sound" -> buildJsonObject {
                put("action", "sound")
                put("name", button.value)
            }
            "obs" -> buildJsonObject {
                put("action", "obs")
                put("op", button.value.ifBlank { "test" })
                put("target", button.arg)
            }
            "audio" -> buildJsonObject {
                put("action", "audio")
                put("op", button.value.ifBlank { "output" })
                put("id", button.arg)
                put("value", amount)
            }
            else -> null
        }
    }

    private fun toggles(button: DeckButton): Boolean = when {
        button.toggle -> true
        button.action == "game_mode" -> true
        button.action == "discord_mute" || button.action == "discord_deafen" -> true
        button.action == "media" && button.value == "play_pause" -> true
        button.action == "obs" && button.value in setOf(
            "record_toggle",
            "stream_toggle",
            "mute_toggle",
        ) -> true
        button.action == "audio" && button.value == "mute" -> true
        else -> false
    }

    // -----------------------------------------------------------------------
    // Media shortcuts
    // -----------------------------------------------------------------------

    fun mediaCommand(command: String) {
        viewModelScope.launch {
            val result = repo.action(
                buildJsonObject {
                    put("action", "media")
                    put("command", command)
                },
            )
            if (!result.ok) toast(result.message.ifBlank { "Medya komutu başarısız." })
        }
    }

    fun volumeStep(direction: String) {
        viewModelScope.launch {
            val result = repo.action(
                buildJsonObject {
                    put("action", "volume")
                    put("direction", direction)
                    put("step", 5)
                },
            )
            if (!result.ok) toast(result.message.ifBlank { "Ses ayarı başarısız." })
        }
    }

    fun toggleMute() {
        viewModelScope.launch {
            val result = repo.action(buildJsonObject { put("action", "mute") })
            if (!result.ok) toast(result.message.ifBlank { "Sessize alma başarısız." })
        }
    }

    fun artUrl(version: Long): String {
        val base = repo.currentBaseUrl
        val currentToken = _ui.value.token
        return buildString {
            append(base)
            append("/api/media/art?v=")
            append(version)
            if (currentToken.isNotBlank()) {
                append("&token=")
                append(java.net.URLEncoder.encode(currentToken, "UTF-8"))
            }
        }
    }

    private fun toast(message: String) {
        _messages.tryEmit(message)
    }

    override fun onCleared() {
        repo.shutdown()
        super.onCleared()
    }
}
