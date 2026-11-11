package com.liquiddeck.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.CloudOff
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.liquiddeck.app.data.QuotaKind
import com.liquiddeck.app.data.QuotaProvider
import com.liquiddeck.app.data.QuotaSnapshot
import com.liquiddeck.app.data.QuotaState
import com.liquiddeck.app.data.QuotaWindow
import com.liquiddeck.app.ui.theme.Blue
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Green
import com.liquiddeck.app.ui.theme.Ink
import com.liquiddeck.app.ui.theme.Orange
import com.liquiddeck.app.ui.theme.Purple
import com.liquiddeck.app.ui.theme.Red
import com.liquiddeck.app.ui.theme.Stroke
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary
import com.liquiddeck.app.ui.theme.Yellow
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.roundToInt

/**
 * Fourth page: remaining AI subscription quota.  Providers are configured and
 * queried on the phone (Z.ai, Claude, OpenRouter, DeepSeek, OpenAI) and the
 * whole list refreshes every three minutes; manual cards cover plans whose
 * vendors do not publish a usage API.
 */
@Composable
fun QuotaPage(
    state: QuotaState,
    onRefresh: () -> Unit,
    onSave: (QuotaProvider) -> Unit,
    onDelete: (String) -> Unit,
) {
    var editing by remember { mutableStateOf<QuotaProvider?>(null) }
    var adding by remember { mutableStateOf(false) }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val landscape = maxWidth > maxHeight
        val columns = when {
            maxWidth >= 1100.dp -> 4
            maxWidth >= 640.dp -> 3
            else -> 2
        }

        Column(Modifier.fillMaxSize()) {
            QuotaHeader(state = state, onRefresh = onRefresh)

            when {
                state.providers.isEmpty() -> EmptyQuota(onAdd = { adding = true })

                landscape -> LazyVerticalGrid(
                    columns = GridCells.Fixed(columns),
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 18.dp, end = 18.dp, top = 2.dp, bottom = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    items(state.providers, key = { it.id }) { provider ->
                        QuotaCard(
                            provider = provider,
                            snapshot = state.snapshots[provider.id],
                            refreshing = provider.id in state.refreshing,
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(CARD_HEIGHT),
                            onClick = { editing = provider },
                        )
                    }
                    item {
                        AddProviderCard(
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(CARD_HEIGHT),
                            onClick = { adding = true },
                        )
                    }
                }

                else -> LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 2.dp, bottom = 16.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    items(state.providers, key = { it.id }) { provider ->
                        QuotaCard(
                            provider = provider,
                            snapshot = state.snapshots[provider.id],
                            refreshing = provider.id in state.refreshing,
                            modifier = Modifier.fillMaxWidth(),
                            onClick = { editing = provider },
                        )
                    }
                    item {
                        AddProviderCard(
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(84.dp),
                            onClick = { adding = true },
                        )
                    }
                }
            }
        }
    }

    val target = editing
    if (adding || target != null) {
        QuotaEditorSheet(
            provider = target,
            onDismiss = {
                adding = false
                editing = null
            },
            onSave = { provider ->
                onSave(provider)
                adding = false
                editing = null
            },
            onDelete = { provider ->
                onDelete(provider.id)
                adding = false
                editing = null
            },
        )
    }
}

// ---------------------------------------------------------------------------
// Header + empty state
// ---------------------------------------------------------------------------

@Composable
private fun QuotaHeader(state: QuotaState, onRefresh: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = 18.dp, end = 8.dp, top = 2.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        SectionLabel("Kota takibi", TextPrimary.copy(alpha = 0.75f))
        Spacer(Modifier.width(10.dp))
        Text(
            "3 dakikada bir yenilenir",
            fontSize = 10.sp,
            color = TextMuted,
            maxLines = 1,
        )
        Spacer(Modifier.weight(1f))
        Text(
            when {
                state.refreshing.isNotEmpty() -> "Yenileniyor…"
                state.providers.isEmpty() -> "Sağlayıcı yok"
                else -> lastUpdated(state)
            },
            fontSize = 10.sp,
            color = if (state.refreshing.isNotEmpty()) Cyan else TextMuted,
            maxLines = 1,
        )
        IconButton(onClick = onRefresh, modifier = Modifier.size(36.dp)) {
            Icon(
                Icons.Rounded.Refresh,
                contentDescription = "Yenile",
                tint = TextMuted,
                modifier = Modifier.size(18.dp),
            )
        }
    }
}

@Composable
private fun EmptyQuota(onAdd: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 18.dp, vertical = 10.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text(
            "Kalan abonelik haklarını buradan takip et",
            style = MaterialTheme.typography.titleMedium,
            color = TextPrimary,
        )
        Spacer(Modifier.height(6.dp))
        Text(
            "Z.ai, Claude, OpenRouter, DeepSeek ve OpenAI anahtarını ekle; " +
                "diğer planları manuel olarak güncelle.",
            style = MaterialTheme.typography.bodyMedium,
            color = TextMuted,
        )
        Spacer(Modifier.height(16.dp))
        Button(
            onClick = onAdd,
            shape = RoundedCornerShape(18.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Blue, contentColor = TextPrimary),
            modifier = Modifier.height(46.dp),
        ) {
            Icon(Icons.Rounded.Add, contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(6.dp))
            Text("Sağlayıcı ekle", style = MaterialTheme.typography.labelLarge)
        }
    }
}

// ---------------------------------------------------------------------------
// Provider card
// ---------------------------------------------------------------------------

@Composable
private fun QuotaCard(
    provider: QuotaProvider,
    snapshot: QuotaSnapshot?,
    refreshing: Boolean,
    modifier: Modifier,
    onClick: () -> Unit,
) {
    val kind = QuotaKind.from(provider.kind)
    val accent = quotaAccent(kind)
    val error = snapshot?.error
    val windows = snapshot?.windows.orEmpty()

    GlassCard(
        modifier = modifier,
        accent = accent,
        onClick = onClick,
        contentPadding = PaddingValues(horizontal = 14.dp, vertical = 11.dp),
    ) {
        // Provider kind and the display name share one line, so the card is
        // almost all dial.
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = kind.label.uppercase(Locale.US),
                fontSize = 9.5.sp,
                letterSpacing = 0.8.sp,
                color = accent,
                maxLines = 1,
            )
            Spacer(Modifier.width(8.dp))
            Text(
                text = provider.label.ifBlank { kind.label },
                fontSize = 13.sp,
                fontWeight = FontWeight.Medium,
                color = TextPrimary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            Spacer(Modifier.weight(1f))
            Text(
                text = when {
                    refreshing -> "yenileniyor"
                    snapshot == null -> "—"
                    else -> clock(snapshot.updatedAt)
                },
                fontSize = 9.sp,
                color = if (refreshing) Cyan else TextMuted,
                maxLines = 1,
            )
        }

        Spacer(Modifier.height(3.dp))

        BoxWithConstraints(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth(),
        ) {
            // Two dials sit side by side at exactly the same size; the pair is
            // sized from whatever height and width the card actually has.
            val spacing = if (windows.size > 1) 6.dp else 0.dp
            val byHeight = maxHeight.value - 24f
            val byWidth = if (windows.size > 1) {
                (maxWidth.value - spacing.value) / 2f - 2f
            } else {
                maxWidth.value * 0.55f
            }
            val dial = minOf(byHeight, byWidth).coerceIn(42f, 96f).dp

            when {
                snapshot == null -> CardNote("Veri bekleniyor…", TextMuted)

                error != null && windows.isEmpty() -> Column(
                    modifier = Modifier.fillMaxSize(),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    Icon(
                        Icons.Rounded.CloudOff,
                        contentDescription = "Bağlantı hatası",
                        tint = Red.copy(alpha = 0.85f),
                        modifier = Modifier.size(22.dp),
                    )
                    Spacer(Modifier.height(4.dp))
                    CardNote(error, Red)
                }

                windows.isEmpty() -> Column(
                    modifier = Modifier.fillMaxSize(),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    Text(
                        text = snapshot.balance ?: budgetHint(kind),
                        fontSize = if (snapshot.balance != null) 22.sp else 9.5.sp,
                        fontWeight = if (snapshot.balance != null) FontWeight.Medium else FontWeight.Normal,
                        color = if (snapshot.balance != null) TextPrimary else TextMuted,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }

                windows.size == 1 -> Row(
                    modifier = Modifier.fillMaxSize(),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    QuotaDial(windows[0], dial)
                }

                else -> Row(
                    modifier = Modifier.fillMaxSize(),
                    horizontalArrangement = Arrangement.spacedBy(spacing, Alignment.CenterHorizontally),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    windows.take(2).forEach { window ->
                        QuotaDial(window, dial, Modifier.weight(1f))
                    }
                }
            }
        }

        if (error != null && windows.isNotEmpty()) {
            Text(
                text = error,
                fontSize = 8.5.sp,
                color = Red,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** One dial plus the short window label underneath: 5 SAAT, 7 GÜN, KREDİ … */
@Composable
private fun QuotaDial(window: QuotaWindow, size: Dp, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        if (window.remaining != null) {
            GaugeRing(
                percent = window.remaining,
                color = quotaColor(window.remaining),
                size = size,
                strokeWidth = 3.dp,
            )
        } else {
            Box(Modifier.size(size), contentAlignment = Alignment.Center) {
                Text(
                    text = window.detail ?: "—",
                    fontSize = (size.value * 0.16f).coerceIn(9f, 14f).sp,
                    fontWeight = FontWeight.Medium,
                    color = TextMuted,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Spacer(Modifier.height(3.dp))
        Text(
            text = windowCaption(window.label),
            fontSize = 9.sp,
            letterSpacing = 0.6.sp,
            color = TextPrimary.copy(alpha = 0.82f),
            maxLines = 1,
        )
        resetShort(window.resetAt)?.let { reset ->
            Text(
                text = reset,
                fontSize = 8.sp,
                color = TextMuted,
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun CardNote(text: String, color: Color) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text(
            text = text,
            fontSize = 9.5.sp,
            color = color,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun AddProviderCard(modifier: Modifier, onClick: () -> Unit) {
    Column(
        modifier = modifier
            .clip(RoundedCornerShape(24.dp))
            .background(Color.White.copy(alpha = 0.02f))
            .border(1.dp, Stroke, RoundedCornerShape(24.dp))
            .clickable(onClick = onClick)
            .padding(14.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Icon(
            Icons.Rounded.Add,
            contentDescription = null,
            tint = Cyan,
            modifier = Modifier.size(24.dp),
        )
        Spacer(Modifier.height(5.dp))
        Text(
            "Sağlayıcı ekle",
            fontSize = 11.sp,
            color = TextMuted,
            maxLines = 1,
        )
    }
}

// ---------------------------------------------------------------------------
// Add / edit sheet
// ---------------------------------------------------------------------------

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun QuotaEditorSheet(
    provider: QuotaProvider?,
    onDismiss: () -> Unit,
    onSave: (QuotaProvider) -> Unit,
    onDelete: (QuotaProvider) -> Unit,
) {
    var kind by remember(provider?.id) { mutableStateOf(QuotaKind.from(provider?.kind)) }
    var label by remember(provider?.id) { mutableStateOf(provider?.label.orEmpty()) }
    var key by remember(provider?.id) { mutableStateOf(provider?.key.orEmpty()) }
    var budget by remember(provider?.id) {
        mutableStateOf(provider?.budget?.let { trimNumber(it) }.orEmpty())
    }
    var manual by remember(provider?.id) {
        mutableFloatStateOf((provider?.manualRemaining ?: 100).toFloat())
    }
    var note by remember(provider?.id) { mutableStateOf(provider?.note.orEmpty()) }
    var error by remember(provider?.id) { mutableStateOf<String?>(null) }

    DeckSheet(onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 22.dp)
                .padding(top = 20.dp, bottom = 26.dp),
        ) {
            Text(
                if (provider == null) "Sağlayıcı ekle" else "Sağlayıcıyı düzenle",
                style = MaterialTheme.typography.headlineMedium,
            )
            Spacer(Modifier.height(3.dp))
            Text(
                "Anahtarlar yalnızca bu cihazda saklanır",
                style = MaterialTheme.typography.bodyMedium,
                color = TextMuted,
            )

            EditorSection("Sağlayıcı") {
                QuotaKind.entries.chunked(2).forEach { row ->
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        row.forEach { option ->
                            Chip(
                                text = option.label,
                                selected = kind == option,
                                modifier = Modifier.weight(1f),
                            ) {
                                kind = option
                                error = null
                            }
                        }
                        if (row.size == 1) Spacer(Modifier.weight(1f))
                    }
                    Spacer(Modifier.height(8.dp))
                }
            }

            EditorSection("Etiket") {
                EditorField(
                    label = "Görünen ad",
                    value = label,
                    placeholder = kind.label,
                    onValueChange = { label = it.take(40) },
                )
            }

            if (kind.needsKey) {
                EditorSection("API anahtarı") {
                    EditorField(
                        label = "Anahtar",
                        value = key,
                        placeholder = "anahtarı buraya yapıştır",
                        keyboardType = KeyboardType.Password,
                        onValueChange = { key = it.trim() },
                    )
                    Hint(kind.keyHint)
                    when (kind) {
                        QuotaKind.CLAUDE -> Hint(
                            "Token süresi dolunca Claude Code'da yeniden giriş yapıp " +
                                "~/.claude/.credentials.json içindeki accessToken'ı buraya güncelle.",
                        )
                        QuotaKind.OPENAI -> Hint(
                            "ChatGPT Plus kotası için API yok. Admin anahtarıyla yalnızca " +
                                "bu ayın organizasyon harcaması okunur.",
                        )
                        QuotaKind.ZAI -> Hint(
                            "Standart API anahtarı Coding Plan kotasını göstermez; " +
                                "kişisel Coding Plan anahtarını kullan.",
                        )
                        else -> Unit
                    }
                }
            }

            if (kind.supportsBudget) {
                EditorSection("Aylık bütçe · opsiyonel") {
                    EditorField(
                        label = "Bütçe (USD)",
                        value = budget,
                        placeholder = "20",
                        keyboardType = KeyboardType.Decimal,
                        onValueChange = { budget = it.filter { ch -> ch.isDigit() || ch == '.' || ch == ',' } },
                    )
                    Hint(
                        "Boş bırakılırsa yalnızca bakiye veya aylık harcama gösterilir. " +
                            "Doldurulursa kalan hak yüzdeye çevrilir.",
                    )
                }
            }

            if (kind == QuotaKind.MANUAL) {
                EditorSection("Kalan hak") {
                    Text(
                        "%${manual.roundToInt()}",
                        fontSize = 30.sp,
                        fontWeight = FontWeight.Medium,
                        color = quotaColor(manual),
                    )
                    Slider(
                        value = manual,
                        onValueChange = { manual = it },
                        valueRange = 0f..100f,
                        steps = 19,
                    )
                    EditorField(
                        label = "Not",
                        value = note,
                        placeholder = "örn. ChatGPT Plus · 12 Kasım",
                        onValueChange = { note = it.take(60) },
                    )
                }
            }

            if (error != null) {
                Spacer(Modifier.height(12.dp))
                Text(error ?: "", style = MaterialTheme.typography.bodyMedium, color = Red)
            }

            Spacer(Modifier.height(22.dp))

            Button(
                onClick = {
                    if (kind.needsKey && key.isBlank()) {
                        error = "API anahtarı gerekli."
                        return@Button
                    }
                    onSave(
                        QuotaProvider(
                            id = provider?.id.orEmpty(),
                            kind = kind.id,
                            label = label.trim(),
                            key = key.trim(),
                            budget = budget.replace(',', '.').toDoubleOrNull()?.takeIf { it > 0 },
                            manualRemaining = manual.roundToInt().coerceIn(0, 100),
                            note = note.trim(),
                        ),
                    )
                },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(52.dp),
                shape = RoundedCornerShape(18.dp),
                colors = ButtonDefaults.buttonColors(
                    containerColor = Blue,
                    contentColor = TextPrimary,
                ),
            ) {
                Text("Kaydet", style = MaterialTheme.typography.labelLarge)
            }

            if (provider != null) {
                Spacer(Modifier.height(10.dp))
                OutlinedButton(
                    onClick = { onDelete(provider) },
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(50.dp),
                    shape = RoundedCornerShape(18.dp),
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = Red),
                ) {
                    Text("Sağlayıcıyı sil", style = MaterialTheme.typography.labelLarge)
                }
            }

            Spacer(Modifier.height(4.dp))
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text("Vazgeç", style = MaterialTheme.typography.labelLarge, color = TextMuted)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

private val CARD_HEIGHT = 150.dp

private fun quotaAccent(kind: QuotaKind): Color = when (kind) {
    QuotaKind.ZAI -> Yellow
    QuotaKind.CLAUDE -> Orange
    QuotaKind.OPENROUTER -> Purple
    QuotaKind.DEEPSEEK -> Blue
    QuotaKind.OPENAI -> Green
    QuotaKind.MANUAL -> Cyan
}

/** Plenty left is green; running out turns red. */
private fun quotaColor(remaining: Float?): Color = when {
    remaining == null -> TextMuted
    remaining < 20f -> Red
    remaining < 50f -> Orange
    else -> Green
}

private fun budgetHint(kind: QuotaKind): String = when (kind) {
    QuotaKind.OPENAI -> "ChatGPT kotası API'de yok; bütçe girip aylık harcamayı izle."
    QuotaKind.DEEPSEEK -> "Bakiye yüzdesi için aylık bütçe gir."
    QuotaKind.OPENROUTER -> "Kredi yüzdesi için aylık bütçe gir."
    QuotaKind.MANUAL -> "Kalan yüzdeyi düzenleyerek güncelle."
    else -> kind.keyHint
}

private fun clock(epochMillis: Long): String {
    if (epochMillis <= 0) return "—"
    return SimpleDateFormat("HH:mm", Locale.getDefault()).format(Date(epochMillis))
}

private fun lastUpdated(state: QuotaState): String {
    val latest = state.snapshots.values.maxOfOrNull { it.updatedAt } ?: 0L
    return if (latest > 0) "${clock(latest)} güncellendi" else "—"
}

/**
 * Short caption under a dial: "5 saat" → "5 SAAT", "7 gün" → "7 GÜN",
 * "Anahtar limiti" → "LİMİT".  The card never prints a sentence.
 */
private fun windowCaption(label: String): String {
    val lowered = label.lowercase(Locale.US)
    Regex("(\\d+)\\s*(saat|dk|dakika|gün)").find(lowered)?.let { match ->
        val count = match.groupValues[1]
        return when (match.groupValues[2]) {
            "dk", "dakika" -> "$count DK"
            "gün" -> "$count GÜN"
            else -> "$count SAAT"
        }
    }
    return when {
        lowered.contains("anahtar") -> "LİMİT"
        lowered.contains("kredi") -> "KREDİ"
        lowered.contains("bakiye") -> "BAKİYE"
        lowered.contains("bütçe") -> "BÜTÇE"
        lowered.contains("kalan") -> "KALAN"
        lowered.contains("süre") -> "SÜRE"
        lowered.contains("token") -> "TOKEN"
        else -> label.uppercase(Locale.US).take(10)
    }
}

/** Compact reset countdown: "42 dk", "3 sa", "2 gün". */
private fun resetShort(resetAt: Long?): String? {
    if (resetAt == null || resetAt <= 0) return null
    val diff = resetAt - System.currentTimeMillis()
    if (diff <= 0) return "şimdi"
    val minutes = diff / 60_000
    return when {
        minutes < 60 -> "$minutes dk"
        minutes < 1_440 -> "${minutes / 60} sa"
        else -> "${minutes / 1_440} gün"
    }
}

private fun trimNumber(value: Double): String =
    if (value % 1.0 == 0.0) value.toInt().toString() else String.format(Locale.US, "%.2f", value)
