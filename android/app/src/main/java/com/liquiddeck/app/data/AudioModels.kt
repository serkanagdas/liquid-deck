package com.liquiddeck.app.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** One render endpoint as reported by the PC (`/api/audio`). */
@Serializable
data class AudioDevice(
    val id: String = "",
    val description: String = "",
    val volume: Int = 0,
    val muted: Boolean = false,
    @SerialName("is_default") val isDefault: Boolean = false,
) {
    /** Short kind label for the card: BLUETOOTH, HDMI, USB, ANALOG … */
    val kind: String
        get() {
            val text = description.lowercase()
            return when {
                listOf("bluetooth", "bluez", "wireless", "airpods", "buds", "wh-1000", "wf-1000")
                    .any { it in text } -> "Bluetooth"
                listOf("headset", "headphone", "arctis", "kulaklık", "hyperx", "steelseries")
                    .any { it in text } -> "Kulaklık"
                listOf("hdmi", "displayport", "display", "monitor", "tv").any { it in text } ->
                    "HDMI"
                listOf("usb", "dac", "scarlett", "focusrite", "audio interface").any { it in text } ->
                    "USB"
                else -> "Analog"
            }
        }
}

@Serializable
data class AudioSnapshot(
    val supported: Boolean = false,
    val backend: String = "",
    val devices: List<AudioDevice> = emptyList(),
    val message: String? = null,
    @SerialName("supports_volume") val supportsVolume: Boolean = false,
) {
    val current: AudioDevice?
        get() = devices.firstOrNull { it.isDefault }
}
