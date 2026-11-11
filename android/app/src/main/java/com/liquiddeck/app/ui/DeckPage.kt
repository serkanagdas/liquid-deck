package com.liquiddeck.app.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Bolt
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.liquiddeck.app.data.Board
import com.liquiddeck.app.data.DeckButton
import com.liquiddeck.app.ui.theme.Card
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Stroke
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary

@Composable
fun DeckPage(ui: UiState, vm: DeckViewModel) {
    val board = ui.activeBoard

    Column(Modifier.fillMaxSize()) {
        BoardTabs(
            boards = ui.boards,
            activeId = ui.activeBoardId,
            editMode = ui.editMode,
            onSelect = vm::selectBoard,
            onToggleEdit = vm::toggleEditMode,
        )

        if (board == null) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(
                        Icons.Rounded.Bolt,
                        contentDescription = null,
                        tint = TextMuted,
                        modifier = Modifier.size(34.dp),
                    )
                    Spacer(Modifier.height(10.dp))
                    Text(
                        if (ui.connection is com.liquiddeck.app.data.Connection.Online) {
                            "Pano ayarı bulunamadı"
                        } else {
                            "Pano bekleniyor…"
                        },
                        style = MaterialTheme.typography.bodyLarge,
                        color = TextMuted,
                    )
                }
            }
        } else {
            val columns = board.columns.coerceIn(2, 8)
            val showSubtitles = ui.config?.settings?.showSubtitles ?: true

            LazyVerticalGrid(
                columns = GridCells.Fixed(columns),
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = 4.dp, bottom = 18.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(
                    items = board.buttons,
                    key = { it.id },
                    span = { button -> GridItemSpan(button.span.coerceIn(1, columns)) },
                ) { button ->
                    DeckKey(
                        button = button,
                        active = ui.isActive(board.id, button.id),
                        busy = "${board.id}/${button.id}" in ui.busyButtons,
                        showSubtitle = showSubtitles,
                        editing = ui.editMode,
                        onPress = {
                            if (ui.editMode) {
                                vm.editKey(board.id, button)
                            } else {
                                vm.press(board, button)
                            }
                        },
                        onEdit = { vm.editKey(board.id, button) },
                    )
                }

                if (ui.editMode) {
                    item(key = "__add-key") {
                        AddKeyTile(onClick = { vm.editKey(board.id, null) })
                    }
                }
            }
        }
    }

    ui.editor?.let { editor ->
        KeyEditorSheet(
            editor = editor,
            boards = ui.boards,
            columns = board?.columns ?: 4,
            onDismiss = vm::closeEditor,
            onUpdate = vm::updateDraft,
            onSave = vm::saveEditor,
            onDelete = vm::deleteEditedKey,
        )
    }
}

@Composable
private fun AddKeyTile(onClick: () -> Unit) {
    val shape = RoundedCornerShape(20.dp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .height(114.dp)
            .clip(shape)
            .background(Cyan.copy(alpha = 0.05f))
            .border(1.dp, Cyan.copy(alpha = 0.35f), shape)
            .clickable(onClick = onClick)
            .padding(12.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(
            Icons.Rounded.Add,
            contentDescription = null,
            tint = Cyan,
            modifier = Modifier.size(26.dp),
        )
        Spacer(Modifier.height(6.dp))
        Text(
            "Tuş ekle",
            style = MaterialTheme.typography.labelLarge,
            color = Cyan,
            maxLines = 1,
        )
    }
}

@Composable
private fun BoardTabs(
    boards: List<Board>,
    activeId: String,
    editMode: Boolean,
    onSelect: (String) -> Unit,
    onToggleEdit: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(end = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        LazyRow(
            modifier = Modifier.weight(1f),
            contentPadding = PaddingValues(start = 16.dp, end = 8.dp, top = 6.dp, bottom = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items(boards, key = { it.id }) { board ->
                val selected = board.id == activeId
                val accent = deckColor(board.accent)
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(50))
                        .background(if (selected) accent.copy(alpha = 0.20f) else Card)
                        .border(
                            1.dp,
                            if (selected) accent.copy(alpha = 0.7f) else Stroke,
                            RoundedCornerShape(50),
                        )
                        .clickable { onSelect(board.id) }
                        .padding(horizontal = 14.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    val icon = iconFor(board.icon)
                    if (icon != null) {
                        Icon(
                            icon,
                            contentDescription = null,
                            tint = if (selected) accent else TextMuted,
                            modifier = Modifier.size(15.dp),
                        )
                        Spacer(Modifier.size(6.dp))
                    }
                    Text(
                        board.name,
                        style = MaterialTheme.typography.labelLarge,
                        color = if (selected) TextPrimary else TextMuted,
                        maxLines = 1,
                    )
                }
            }
        }

        Row(
            modifier = Modifier
                .clip(RoundedCornerShape(50))
                .background(if (editMode) Cyan.copy(alpha = 0.20f) else Card)
                .border(
                    1.dp,
                    if (editMode) Cyan.copy(alpha = 0.7f) else Stroke,
                    RoundedCornerShape(50),
                )
                .clickable { onToggleEdit() }
                .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                if (editMode) Icons.Rounded.Check else Icons.Rounded.Edit,
                contentDescription = "Tuşları düzenle",
                tint = if (editMode) Cyan else TextMuted,
                modifier = Modifier.size(15.dp),
            )
            if (editMode) {
                Spacer(Modifier.size(6.dp))
                Text(
                    "Bitti",
                    style = MaterialTheme.typography.labelLarge,
                    color = TextPrimary,
                    maxLines = 1,
                )
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun DeckKey(
    button: DeckButton,
    active: Boolean,
    busy: Boolean,
    showSubtitle: Boolean,
    editing: Boolean,
    onPress: () -> Unit,
    onEdit: () -> Unit,
) {
    val accent = deckColor(button.color)
    val shape = RoundedCornerShape(20.dp)
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (pressed) 0.95f else 1f,
        label = "deck-key-press",
    )
    val haptics = LocalHapticFeedback.current

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(114.dp)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
                alpha = if (busy) 0.62f else 1f
            }
            .clip(shape)
            .background(
                if (active) accent.copy(alpha = 0.26f) else accent.copy(alpha = 0.09f),
            )
            .border(
                if (editing) 1.5.dp else 1.dp,
                when {
                    editing -> Cyan.copy(alpha = 0.55f)
                    active -> accent.copy(alpha = 0.85f)
                    else -> accent.copy(alpha = 0.24f)
                },
                shape,
            )
            .combinedClickable(
                interactionSource = interaction,
                indication = null,
                enabled = !busy,
                onLongClick = {
                    haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                    onEdit()
                },
                onClick = {
                    haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                    onPress()
                },
            )
            .padding(horizontal = 9.dp, vertical = 11.dp),
    ) {
        Column(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.SpaceBetween,
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                DeckIcon(button = button, active = active, accent = accent)
                Spacer(Modifier.weight(1f))
                if (editing) {
                    Icon(
                        Icons.Rounded.Edit,
                        contentDescription = "Tuşu düzenle",
                        tint = Cyan,
                        modifier = Modifier.size(15.dp),
                    )
                } else if (active) {
                    Box(
                        Modifier
                            .size(7.dp)
                            .clip(RoundedCornerShape(50))
                            .background(accent),
                    )
                }
            }

            Column {
                Text(
                    if (active && button.activeLabel.isNotBlank()) button.activeLabel else button.label,
                    style = MaterialTheme.typography.titleMedium.copy(
                        fontSize = 13.sp,
                        lineHeight = 16.sp,
                    ),
                    color = TextPrimary,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                if (showSubtitle && button.subtitle.isNotBlank()) {
                    Spacer(Modifier.height(2.dp))
                    Text(
                        button.subtitle,
                        style = MaterialTheme.typography.bodyMedium.copy(
                            fontSize = 9.5.sp,
                            lineHeight = 11.5.sp,
                        ),
                        color = TextMuted,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

@Composable
private fun DeckIcon(button: DeckButton, active: Boolean, accent: Color) {
    val name = if (active && button.activeIcon.isNotBlank()) button.activeIcon else button.icon
    val vector = iconFor(name)

    if (vector != null) {
        Icon(
            vector,
            contentDescription = null,
            tint = if (active) accent else accent.copy(alpha = 0.9f),
            modifier = Modifier.size(25.dp),
        )
    } else if (name.length in 1..4) {
        Text(
            name,
            fontSize = 21.sp,
            color = TextPrimary,
        )
    } else {
        Icon(
            Icons.Rounded.Bolt,
            contentDescription = null,
            tint = accent,
            modifier = Modifier.size(25.dp),
        )
    }
}
