package com.liquiddeck.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import com.liquiddeck.app.data.Board
import com.liquiddeck.app.data.DeckButton
import com.liquiddeck.app.ui.theme.Blue
import com.liquiddeck.app.ui.theme.Card
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Ink
import com.liquiddeck.app.ui.theme.Red
import com.liquiddeck.app.ui.theme.Stroke
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary

private val EDITOR_COLORS = listOf(
    "cyan", "blue", "orange", "violet", "pink", "green", "red", "yellow", "slate",
)

private val EDITOR_ACTIONS = listOf(
    "media",
    "volume",
    "mute",
    "key",
    "launch",
    "terminal",
    "url",
    "sound",
    "script",
    "obs",
    "audio",
    "board",
    "panel",
    "discord_mute",
    "discord_deafen",
    "game_mode",
    "none",
)

private fun actionLabel(action: String): String = when (action) {
    "media" -> "Medya kontrolü"
    "volume" -> "Sistem sesi"
    "mute" -> "Sessize al"
    "key" -> "Klavye kısayolu"
    "launch" -> "Uygulama aç"
    "terminal" -> "Terminal aç"
    "url" -> "Bağlantı aç"
    "sound" -> "Ses çal"
    "script" -> "Betik çalıştır"
    "obs" -> "OBS"
    "audio" -> "Ses mikseri"
    "board" -> "Pano değiştir"
    "panel" -> "Panel aç"
    "discord_mute" -> "Discord · mikrofon"
    "discord_deafen" -> "Discord · kulaklık"
    "game_mode" -> "Oyun modu"
    "none" -> "Eylem yok"
    else -> action
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun KeyEditorSheet(
    editor: KeyEditor,
    boards: List<Board>,
    columns: Int,
    onDismiss: () -> Unit,
    onUpdate: ((DeckButton) -> DeckButton) -> Unit,
    onSave: () -> Unit,
    onDelete: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val draft = editor.draft
    val boardName = boards.firstOrNull { it.id == editor.boardId }?.name.orEmpty()

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = Ink,
        contentColor = TextPrimary,
        dragHandle = null,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .imePadding()
                .padding(horizontal = 22.dp)
                .padding(top = 20.dp, bottom = 26.dp),
        ) {
            Text(
                if (editor.isNew) "Yeni tuş" else "Tuşu düzenle",
                style = MaterialTheme.typography.headlineMedium,
            )
            Spacer(Modifier.height(3.dp))
            Text(
                if (boardName.isBlank()) "Pano" else "$boardName panosu",
                style = MaterialTheme.typography.bodyMedium,
                color = TextMuted,
            )

            EditorSection("Kimlik") {
                EditorField(
                    label = "Etiket",
                    value = draft.label,
                    placeholder = "Örn. Ses kıs",
                    onValueChange = { value -> onUpdate { it.copy(label = value.take(48)) } },
                )
                Spacer(Modifier.height(10.dp))
                EditorField(
                    label = "Alt yazı",
                    value = draft.subtitle,
                    placeholder = "opsiyonel",
                    onValueChange = { value -> onUpdate { it.copy(subtitle = value.take(64)) } },
                )
                if (draft.toggle) {
                    Spacer(Modifier.height(10.dp))
                    EditorField(
                        label = "Açıkken etiket",
                        value = draft.activeLabel,
                        placeholder = draft.label,
                        onValueChange = { value -> onUpdate { it.copy(activeLabel = value.take(48)) } },
                    )
                }
            }

            EditorSection("Renk") {
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    EDITOR_COLORS.forEach { name ->
                        val color = deckColor(name)
                        val selected = draft.color == name
                        Box(
                            modifier = Modifier
                                .size(32.dp)
                                .clip(CircleShape)
                                .background(color.copy(alpha = if (selected) 0.45f else 0.16f))
                                .border(
                                    if (selected) 2.dp else 1.dp,
                                    if (selected) color else Stroke,
                                    CircleShape,
                                )
                                .clickable { onUpdate { it.copy(color = name) } },
                        )
                    }
                }
            }

            EditorSection("Simge") {
                LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(deckIconNames, key = { it }) { name ->
                        val icon = iconFor(name)
                        val selected = draft.icon == name
                        Box(
                            modifier = Modifier
                                .size(44.dp)
                                .clip(RoundedCornerShape(14.dp))
                                .background(if (selected) Cyan.copy(alpha = 0.18f) else Card)
                                .border(
                                    if (selected) 1.5.dp else 1.dp,
                                    if (selected) Cyan.copy(alpha = 0.7f) else Stroke,
                                    RoundedCornerShape(14.dp),
                                )
                                .clickable { onUpdate { it.copy(icon = name) } },
                            contentAlignment = Alignment.Center,
                        ) {
                            if (icon != null) {
                                Icon(
                                    icon,
                                    contentDescription = name,
                                    tint = if (selected) Cyan else TextMuted,
                                    modifier = Modifier.size(22.dp),
                                )
                            } else {
                                Text(name, style = MaterialTheme.typography.labelSmall, color = TextMuted)
                            }
                        }
                    }
                }
            }

            EditorSection("Eylem") {
                EDITOR_ACTIONS.chunked(2).forEach { row ->
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        row.forEach { action ->
                            Chip(
                                text = actionLabel(action),
                                selected = draft.action == action,
                                modifier = Modifier.weight(1f),
                            ) {
                                onUpdate { it.copy(action = action, value = "", arg = "") }
                            }
                        }
                        if (row.size == 1) Spacer(Modifier.weight(1f))
                    }
                    Spacer(Modifier.height(8.dp))
                }
                Spacer(Modifier.height(2.dp))
                ActionFields(draft = draft, boards = boards, columns = columns, onUpdate = onUpdate)
            }

            EditorSection("Yerleşim") {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Genişlik", style = MaterialTheme.typography.bodyLarge, color = TextPrimary)
                    Spacer(Modifier.weight(1f))
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        (1..columns.coerceIn(1, 4)).forEach { span ->
                            Chip(
                                text = "$span",
                                selected = draft.span == span,
                            ) { onUpdate { it.copy(span = span) } }
                        }
                    }
                }
                Spacer(Modifier.height(14.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text("Anahtar", style = MaterialTheme.typography.bodyLarge, color = TextPrimary)
                        Spacer(Modifier.height(2.dp))
                        Text(
                            "Açık/kapalı durumu sunucuda saklanır",
                            style = MaterialTheme.typography.bodyMedium,
                            color = TextMuted,
                        )
                    }
                    Switch(
                        checked = draft.toggle,
                        onCheckedChange = { value -> onUpdate { it.copy(toggle = value) } },
                        colors = SwitchDefaults.colors(
                            checkedThumbColor = TextPrimary,
                            checkedTrackColor = Blue,
                            uncheckedThumbColor = TextMuted,
                            uncheckedTrackColor = Card,
                            uncheckedBorderColor = Stroke,
                        ),
                    )
                }
            }

            Spacer(Modifier.height(22.dp))

            if (editor.error != null) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(14.dp))
                        .background(Red.copy(alpha = 0.12f))
                        .border(1.dp, Red.copy(alpha = 0.45f), RoundedCornerShape(14.dp))
                        .padding(horizontal = 14.dp, vertical = 10.dp),
                ) {
                    Text(
                        editor.error,
                        style = MaterialTheme.typography.bodyMedium,
                        color = Red,
                    )
                }
                Spacer(Modifier.height(12.dp))
            }

            Button(
                onClick = onSave,
                enabled = !editor.saving,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(52.dp),
                shape = RoundedCornerShape(18.dp),
                colors = ButtonDefaults.buttonColors(
                    containerColor = Blue,
                    contentColor = TextPrimary,
                ),
            ) {
                Text(
                    if (editor.saving) "Kaydediliyor…" else "Kaydet",
                    style = MaterialTheme.typography.labelLarge,
                )
            }

            if (!editor.isNew) {
                Spacer(Modifier.height(10.dp))
                OutlinedButton(
                    onClick = onDelete,
                    enabled = !editor.saving,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(50.dp),
                    shape = RoundedCornerShape(18.dp),
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = Red),
                ) {
                    Text("Tuşu sil", style = MaterialTheme.typography.labelLarge)
                }
            }

            Spacer(Modifier.height(4.dp))
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text("Vazgeç", style = MaterialTheme.typography.labelLarge, color = TextMuted)
            }
        }
    }
}

@Composable
internal fun EditorSection(title: String, content: @Composable () -> Unit) {
    Spacer(Modifier.height(20.dp))
    SectionLabel(title)
    Spacer(Modifier.height(10.dp))
    content()
}

@Composable
private fun ActionFields(
    draft: DeckButton,
    boards: List<Board>,
    columns: Int,
    onUpdate: ((DeckButton) -> DeckButton) -> Unit,
) {
    when (draft.action) {
        "media" -> ChoiceChips(
            options = listOf(
                "play_pause" to "Oynat / duraklat",
                "next" to "Sonraki",
                "previous" to "Önceki",
                "stop" to "Durdur",
            ),
            selected = draft.value.ifBlank { "play_pause" },
        ) { value -> onUpdate { it.copy(value = value) } }

        "volume" -> {
            ChoiceChips(
                options = listOf("up" to "Artır", "down" to "Azalt"),
                selected = draft.value.ifBlank { "up" },
            ) { value -> onUpdate { it.copy(value = value) } }
            Spacer(Modifier.height(10.dp))
            StepChips(draft.amount) { amount -> onUpdate { it.copy(amount = amount) } }
        }

        "key" -> {
            EditorField(
                label = "Kısayol",
                value = draft.value,
                placeholder = "ctrl+shift+m",
                keyboardType = KeyboardType.Ascii,
                onValueChange = { value -> onUpdate { it.copy(value = value.take(48)) } },
            )
            Hint("Linux'ta xdotool (X11) ya da wtype (Wayland) kurulu olmalı.")
        }

        "launch" -> ChoiceChips(
            options = listOf(
                "terminal" to "Terminal",
                "browser" to "Tarayıcı",
                "files" to "Dosyalar",
                "calculator" to "Hesap makinesi",
                "editor" to "Editör",
            ),
            selected = draft.value.ifBlank { "terminal" },
        ) { value -> onUpdate { it.copy(value = value) } }

        "url" -> EditorField(
            label = "Adres",
            value = draft.value,
            placeholder = "https://example.com",
            keyboardType = KeyboardType.Uri,
            onValueChange = { value -> onUpdate { it.copy(value = value.take(2048)) } },
        )

        "sound" -> {
            EditorField(
                label = "Ses dosyası",
                value = draft.value,
                placeholder = "click.wav",
                onValueChange = { value -> onUpdate { it.copy(value = value.take(128)) } },
            )
            Hint("PC'deki ~/.config/liquid-deck/sounds klasöründen bir dosya adı.")
        }

        "script" -> {
            EditorField(
                label = "Betik adı",
                value = draft.value,
                placeholder = "ornek.sh",
                onValueChange = { value -> onUpdate { it.copy(value = value.take(128)) } },
            )
            Hint("Sunucu --scripts klasörüyle başlatılmadıysa betikler kapalıdır.")
        }

        "obs" -> {
            ChoiceChips(
                options = listOf(
                    "record_toggle" to "Kayıt",
                    "stream_toggle" to "Yayın",
                    "mute_toggle" to "Mikrofon",
                    "scene" to "Sahne",
                    "source" to "Kaynak",
                ),
                selected = draft.value.ifBlank { "record_toggle" },
            ) { value -> onUpdate { it.copy(value = value) } }
            if (draft.value == "scene" || draft.value == "source") {
                Spacer(Modifier.height(10.dp))
                EditorField(
                    label = if (draft.value == "scene") "Sahne adı" else "Kaynak adı",
                    value = draft.arg,
                    placeholder = "Main",
                    onValueChange = { value -> onUpdate { it.copy(arg = value.take(160)) } },
                )
            }
        }

        "audio" -> {
            ChoiceChips(
                options = listOf(
                    "output" to "Çıkış",
                    "default" to "Varsayılan yap",
                    "mute" to "Sessiz",
                    "volume" to "Seviye",
                    "app_volume" to "Uygulama",
                ),
                selected = draft.value.ifBlank { "output" },
            ) { value -> onUpdate { it.copy(value = value) } }
            if (draft.value == "volume" || draft.value == "app_volume") {
                Spacer(Modifier.height(10.dp))
                StepChips(draft.amount) { amount -> onUpdate { it.copy(amount = amount) } }
            }
            if (draft.value == "volume" || draft.value == "mute" || draft.value == "default") {
                Spacer(Modifier.height(10.dp))
                EditorField(
                    label = "Çıkış kimliği",
                    value = draft.arg,
                    placeholder = "boş = varsayılan",
                    onValueChange = { value -> onUpdate { it.copy(arg = value.take(160)) } },
                )
            }
        }

        "board" -> {
            if (boards.isEmpty()) {
                Hint("Başka bir pano yok.")
            } else {
                ChoiceChips(
                    options = boards.map { it.id to it.name },
                    selected = draft.value,
                ) { value -> onUpdate { it.copy(value = value) } }
            }
        }

        "panel" -> ChoiceChips(
            options = listOf(
                "settings" to "Ayarlar",
                "editor" to "Düzenleyici",
                "audio" to "Ses mikseri",
            ),
            selected = draft.value.ifBlank { "settings" },
        ) { value -> onUpdate { it.copy(value = value) } }

        "discord_mute", "discord_deafen" ->
            Hint("Discord penceresine kısayol gönderir; Linux'ta xdotool ya da wtype gerekir.")

        "game_mode" ->
            Hint("Sunucuda LIQUID_DECK_GAME_MODE_ON / OFF ortam değişkeni tanımlı olmalı.")

        "terminal" -> Hint("PC'de varsayılan terminali açar.")

        "mute" -> Hint("Sistem çıkışını sessize alır ya da geri açar.")

        else -> Hint("Bu tuş hiçbir şey yapmaz.")
    }

    if (draft.action != "none" && draft.action != "mute") {
        Spacer(Modifier.height(6.dp))
        Text(
            "${columns.coerceIn(1, 4)} sütunlu panoda ${draft.span.coerceIn(1, columns)} hücre kaplar.",
            style = MaterialTheme.typography.bodyMedium,
            color = TextMuted,
        )
    }
}

@Composable
private fun ChoiceChips(
    options: List<Pair<String, String>>,
    selected: String,
    onSelect: (String) -> Unit,
) {
    options.chunked(2).forEach { row ->
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            row.forEach { (value, label) ->
                Chip(
                    text = label,
                    selected = selected == value,
                    modifier = Modifier.weight(1f),
                ) { onSelect(value) }
            }
            if (row.size == 1) Spacer(Modifier.weight(1f))
        }
        Spacer(Modifier.height(8.dp))
    }
}

@Composable
private fun StepChips(amount: Int, onSelect: (Int) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        listOf(2, 5, 10, 15, 25).forEach { step ->
            Chip(text = "%$step", selected = amount == step) { onSelect(step) }
        }
    }
}

@Composable
internal fun Chip(
    text: String,
    selected: Boolean,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    Box(
        modifier = modifier
            .clip(RoundedCornerShape(50))
            .background(if (selected) Cyan.copy(alpha = 0.20f) else Card)
            .border(
                if (selected) 1.5.dp else 1.dp,
                if (selected) Cyan.copy(alpha = 0.7f) else Stroke,
                RoundedCornerShape(50),
            )
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text,
            style = MaterialTheme.typography.labelLarge,
            color = if (selected) TextPrimary else TextMuted,
            maxLines = 1,
        )
    }
}

@Composable
internal fun EditorField(
    label: String,
    value: String,
    placeholder: String,
    keyboardType: KeyboardType = KeyboardType.Text,
    onValueChange: (String) -> Unit,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = Modifier.fillMaxWidth(),
        label = { Text(label) },
        placeholder = { Text(placeholder) },
        singleLine = true,
        shape = RoundedCornerShape(16.dp),
        keyboardOptions = KeyboardOptions(keyboardType = keyboardType, imeAction = ImeAction.Next),
        colors = editorFieldColors(),
    )
}

@Composable
internal fun Hint(text: String) {
    Spacer(Modifier.height(6.dp))
    Text(text, style = MaterialTheme.typography.bodyMedium, color = TextMuted)
}

@Composable
private fun editorFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedTextColor = TextPrimary,
    unfocusedTextColor = TextPrimary,
    focusedContainerColor = Card,
    unfocusedContainerColor = Card,
    focusedBorderColor = Cyan.copy(alpha = 0.7f),
    unfocusedBorderColor = Stroke,
    focusedLabelColor = Cyan,
    unfocusedLabelColor = TextMuted,
    cursorColor = Cyan,
    focusedPlaceholderColor = TextMuted.copy(alpha = 0.6f),
    unfocusedPlaceholderColor = TextMuted.copy(alpha = 0.6f),
)
