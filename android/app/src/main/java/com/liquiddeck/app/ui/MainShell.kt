package com.liquiddeck.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.DataUsage
import androidx.compose.material.icons.rounded.GridView
import androidx.compose.material.icons.rounded.Monitor
import androidx.compose.material.icons.rounded.MusicNote
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.automirrored.rounded.VolumeUp
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.liquiddeck.app.ui.theme.Amoled
import com.liquiddeck.app.ui.theme.Blue
import com.liquiddeck.app.ui.theme.Card
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Stroke
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun MainShell(ui: UiState, vm: DeckViewModel) {
    // The five pages live in a pager, so a horizontal swipe moves between them
    // with the same gesture as the dock, and the dock follows the swipe.
    val pagerState = rememberPagerState(initialPage = 0) { PAGE_COUNT }
    val page = pagerState.currentPage
    val scope = rememberCoroutineScope()
    val goTo: (Int) -> Unit = { target ->
        val clamped = target.coerceIn(0, PAGE_COUNT - 1)
        if (clamped != pagerState.currentPage) {
            scope.launch { pagerState.animateScrollToPage(clamped) }
        }
    }
    val quotaVm: QuotaViewModel = viewModel()
    val quota by quotaVm.state.collectAsStateWithLifecycle()
    val audio by vm.audio.collectAsStateWithLifecycle()

    // The audio page is the only consumer of the endpoint list, so it polls
    // only while it is the visible page.
    LaunchedEffect(page) { vm.setAudioPageVisible(page == AUDIO_PAGE) }
    DisposableEffect(Unit) {
        onDispose { vm.setAudioPageVisible(false) }
    }

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(Amoled),
    ) {
        // Landscape head units and short tablets get a thinner chrome so the
        // data keeps the space; portrait phones keep the roomier version.
        val compact = maxHeight < 520.dp

        Column(Modifier.fillMaxSize()) {
            StatusBar(ui, compact = compact, onSettings = { vm.setShowSettings(true) })

            HorizontalPager(
                state = pagerState,
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
            ) { index ->
                when (index) {
                    0 -> SystemPage(
                        ui = ui,
                        onOpenMedia = { goTo(1) },
                        onMediaCommand = vm::mediaCommand,
                        artUrl = vm::artUrl,
                    )

                    1 -> MediaPage(ui = ui, vm = vm)

                    2 -> AudioPage(
                        state = audio,
                        hapticsEnabled = ui.haptics,
                        onRefresh = vm::refreshAudio,
                        onSelect = vm::selectAudio,
                    )

                    3 -> DeckPage(ui = ui, vm = vm)

                    else -> QuotaPage(
                        state = quota,
                        onRefresh = quotaVm::refreshNow,
                        onSave = quotaVm::saveProvider,
                        onDelete = quotaVm::removeProvider,
                    )
                }
            }

            Dock(selected = page, compact = compact, onSelect = goTo)
        }
    }
}

private const val PAGE_COUNT = 5
private const val AUDIO_PAGE = 2

@Composable
private fun StatusBar(ui: UiState, compact: Boolean, onSettings: () -> Unit) {
    val clock = rememberClock()
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                start = 18.dp,
                end = 8.dp,
                top = if (compact) 2.dp else 6.dp,
                bottom = if (compact) 2.dp else 4.dp,
            ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            if (compact) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "LIQUID DECK",
                        style = MaterialTheme.typography.labelSmall,
                        color = Cyan,
                    )
                    Spacer(Modifier.width(10.dp))
                    Text(
                        ui.host.ifBlank { "Sunucu bekleniyor" },
                        style = MaterialTheme.typography.bodyMedium,
                        color = TextMuted,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            } else {
                Text(
                    "LIQUID DECK",
                    style = MaterialTheme.typography.labelSmall,
                    color = Cyan,
                )
                Text(
                    ui.host.ifBlank { "Sunucu bekleniyor" },
                    style = MaterialTheme.typography.bodyMedium,
                    color = TextMuted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }

        Text(
            clock,
            style = MaterialTheme.typography.titleMedium,
            color = TextPrimary,
        )
        IconButton(
            onClick = onSettings,
            modifier = if (compact) Modifier.size(36.dp) else Modifier,
        ) {
            Icon(
                Icons.Rounded.Settings,
                contentDescription = "Ayarlar",
                tint = TextMuted,
                modifier = Modifier.size(if (compact) 19.dp else 22.dp),
            )
        }
    }
}

@Composable
private fun Dock(selected: Int, compact: Boolean, onSelect: (Int) -> Unit) {
    val items: List<Triple<String, ImageVector, Int>> = listOf(
        Triple("Sistem", Icons.Rounded.Monitor, 0),
        Triple("Medya", Icons.Rounded.MusicNote, 1),
        Triple("Ses", Icons.AutoMirrored.Rounded.VolumeUp, 2),
        Triple("Deck", Icons.Rounded.GridView, 3),
        Triple("Kota", Icons.Rounded.DataUsage, 4),
    )

    Box(
        modifier = Modifier.fillMaxWidth(),
        contentAlignment = Alignment.Center,
    ) {
        Row(
            modifier = Modifier
                .padding(
                    horizontal = 16.dp,
                    vertical = if (compact) 4.dp else 8.dp,
                )
                .widthIn(max = 430.dp)
                .fillMaxWidth()
                .clip(RoundedCornerShape(if (compact) 20.dp else 26.dp))
                .background(Card)
                .border(1.dp, Stroke, RoundedCornerShape(if (compact) 20.dp else 26.dp))
                .padding(if (compact) 5.dp else 6.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            items.forEach { (label, icon, index) ->
                val active = selected == index
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .clip(RoundedCornerShape(if (compact) 15.dp else 20.dp))
                        .background(if (active) Blue.copy(alpha = 0.18f) else Color.Transparent)
                        .clickable { onSelect(index) }
                        .padding(vertical = if (compact) 5.dp else 7.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Icon(
                        icon,
                        contentDescription = label,
                        tint = if (active) Cyan else TextMuted,
                        modifier = Modifier.size(if (compact) 19.dp else 22.dp),
                    )
                    Spacer(Modifier.height(2.dp))
                    Text(
                        label,
                        style = MaterialTheme.typography.labelSmall,
                        color = if (active) TextPrimary else TextMuted,
                    )
                }
            }
        }
    }
}

@Composable
private fun rememberClock(): String {
    var value by remember { mutableStateOf("") }
    LaunchedEffect(Unit) {
        while (true) {
            value = SimpleDateFormat("dd MMM · HH:mm", Locale("tr", "TR")).format(Date())
            delay(60_000 - System.currentTimeMillis() % 60_000)
        }
    }
    return value
}
