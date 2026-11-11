package com.liquiddeck.app.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// ---------------------------------------------------------------------------
// Wire models. Every model mirrors the Rust server's JSON, with defaults so a
// newer server can add fields without breaking an older app.
// ---------------------------------------------------------------------------

@Serializable
data class Metrics(
    val ts: Long = 0,
    val cpu: Float = 0f,
    @SerialName("cpu_cores") val cpuCores: Int = 0,
    @SerialName("ram_used") val ramUsed: Long = 0,
    @SerialName("ram_total") val ramTotal: Long = 0,
    @SerialName("ram_pct") val ramPct: Float = 0f,
    val temp: Float? = null,
    @SerialName("temp_source") val tempSource: String? = null,
    @SerialName("cpu_power") val cpuPower: Float? = null,
    val cpus: List<Int> = emptyList(),
    @SerialName("cpu_fans") val cpuFans: List<Int?> = emptyList(),
    @SerialName("cpu_fan_pct") val cpuFanPct: Int? = null,
    @SerialName("cpu_chip") val cpuChip: String? = null,
    val gpu: GpuMetrics? = null,
    @SerialName("net_rx") val netRx: Double = 0.0,
    @SerialName("net_tx") val netTx: Double = 0.0,
    @SerialName("net_rx_total") val netRxTotal: Long = 0,
    @SerialName("net_tx_total") val netTxTotal: Long = 0,
    val ping: Float? = null,
    @SerialName("ping_ts") val pingTs: Long = 0,
    val uptime: Long = 0,
    val load: List<Float> = emptyList(),
    val host: String = "",
    val os: String = "",
    val iface: String = "",
    val media: MediaSnapshot? = null,
)

@Serializable
data class GpuMetrics(
    val supported: Boolean = false,
    val name: String? = null,
    val usage: Int? = null,
    val temp: Int? = null,
    @SerialName("memory_used") val memoryUsed: Long? = null,
    @SerialName("memory_total") val memoryTotal: Long? = null,
    @SerialName("memory_pct") val memoryPct: Float? = null,
    @SerialName("clock_mhz") val clockMhz: Int? = null,
    @SerialName("mem_clock_mhz") val memClockMhz: Int? = null,
    val pstate: Int? = null,
    @SerialName("power_watts") val powerWatts: Float? = null,
    @SerialName("power_limit_watts") val powerLimitWatts: Float? = null,
    @SerialName("fan_pct") val fanPct: Int? = null,
    @SerialName("fan_rpm") val fanRpm: Int? = null,
    val status: String? = null,
)

@Serializable
data class MediaSnapshot(
    val app: String = "",
    val title: String = "",
    val artist: String = "",
    val album: String = "",
    val playing: Boolean = false,
    @SerialName("pos_ms") val posMs: Long = 0,
    @SerialName("dur_ms") val durMs: Long = 0,
    @SerialName("can_seek") val canSeek: Boolean = false,
    @SerialName("art_version") val artVersion: Long = 0,
)

@Serializable
data class DeckConfig(
    val version: Int = 1,
    val boards: List<Board> = emptyList(),
    val settings: DeckSettings = DeckSettings(),
)

@Serializable
data class DeckSettings(
    @SerialName("default_board") val defaultBoard: String = "",
    val haptics: Boolean = true,
    @SerialName("show_subtitles") val showSubtitles: Boolean = true,
    val obs: ObsSettings = ObsSettings(),
)

@Serializable
data class ObsSettings(
    val url: String = "",
    val password: String = "",
)

@Serializable
data class Board(
    val id: String = "",
    val name: String = "Board",
    val icon: String = "grid",
    val accent: String = "#64d2ff",
    val columns: Int = 4,
    val buttons: List<DeckButton> = emptyList(),
)

@Serializable
data class DeckButton(
    val id: String = "",
    val label: String = "Button",
    val subtitle: String = "",
    val icon: String = "bolt",
    @SerialName("active_label") val activeLabel: String = "",
    @SerialName("active_icon") val activeIcon: String = "",
    val color: String = "cyan",
    val action: String = "none",
    val value: String = "",
    val arg: String = "",
    val amount: Int = 5,
    val toggle: Boolean = false,
    val span: Int = 1,
)

@Serializable
data class DeckState(
    val toggles: Map<String, Boolean> = emptyMap(),
    @SerialName("active_board") val activeBoard: String = "",
    @SerialName("updated_at") val updatedAt: Long = 0,
)

// ---------------------------------------------------------------------------
// Protocol
// ---------------------------------------------------------------------------

sealed interface ServerEvent {
    data class Hello(
        val version: String,
        val metrics: Metrics?,
        val config: DeckConfig?,
        val state: DeckState?,
    ) : ServerEvent

    data class Sample(val metrics: Metrics) : ServerEvent

    data class ConfigUpdate(val config: DeckConfig, val state: DeckState) : ServerEvent

    data class StateUpdate(val state: DeckState) : ServerEvent

    data class ActionResult(
        val requestId: String?,
        val result: com.liquiddeck.app.data.ActionResult,
    ) : ServerEvent

    data object Unknown : ServerEvent
}

sealed interface Connection {
    data object Connecting : Connection
    data object Online : Connection
    data class Offline(val reason: String? = null) : Connection
}

data class ActionResult(
    val ok: Boolean,
    val message: String,
    val output: String? = null,
)

data class ConfigSave(
    val ok: Boolean,
    val message: String,
    val config: DeckConfig?,
)

data class ProbeResult(
    val ok: Boolean,
    val message: String,
    val version: String? = null,
    val host: String? = null,
    val authRequired: Boolean = false,
)
