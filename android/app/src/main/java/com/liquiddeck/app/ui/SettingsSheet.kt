package com.liquiddeck.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import com.liquiddeck.app.data.Connection
import com.liquiddeck.app.ui.theme.Blue
import com.liquiddeck.app.ui.theme.Card
import com.liquiddeck.app.ui.theme.Cyan
import com.liquiddeck.app.ui.theme.Ink
import com.liquiddeck.app.ui.theme.Red
import com.liquiddeck.app.ui.theme.Stroke
import com.liquiddeck.app.ui.theme.TextMuted
import com.liquiddeck.app.ui.theme.TextPrimary

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsSheet(
    ui: UiState,
    onDismiss: () -> Unit,
    onSave: (String, String, Boolean, Boolean) -> Unit,
    onReconnect: () -> Unit,
    onDisconnect: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var host by rememberSaveable(ui.host) { mutableStateOf(ui.host) }
    var token by rememberSaveable(ui.token) { mutableStateOf(ui.token) }
    var keepAwake by rememberSaveable(ui.keepAwake) { mutableStateOf(ui.keepAwake) }
    var haptics by rememberSaveable(ui.haptics) { mutableStateOf(ui.haptics) }

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
                .padding(horizontal = 22.dp)
                .padding(top = 22.dp, bottom = 24.dp),
        ) {
            Text("Ayarlar", style = MaterialTheme.typography.headlineMedium)
            Spacer(Modifier.height(4.dp))
            Text(
                "Bağlantı ve panel davranışı",
                style = MaterialTheme.typography.bodyMedium,
                color = TextMuted,
            )

            Spacer(Modifier.height(22.dp))

            OutlinedTextField(
                value = host,
                onValueChange = { host = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Sunucu adresi") },
                placeholder = { Text("192.168.1.50:8787") },
                singleLine = true,
                shape = RoundedCornerShape(16.dp),
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Uri,
                    imeAction = ImeAction.Next,
                ),
                colors = settingsFieldColors(),
            )

            Spacer(Modifier.height(10.dp))

            OutlinedTextField(
                value = token,
                onValueChange = { token = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Token") },
                placeholder = { Text("opsiyonel") },
                singleLine = true,
                shape = RoundedCornerShape(16.dp),
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Password,
                    imeAction = ImeAction.Done,
                ),
                colors = settingsFieldColors(),
            )

            Spacer(Modifier.height(18.dp))

            SwitchRow(
                title = "Ekranı açık tut",
                subtitle = "Panel açıkken telefon uykuya geçmez",
                checked = keepAwake,
                onCheckedChange = { keepAwake = it },
            )
            Spacer(Modifier.height(12.dp))
            SwitchRow(
                title = "Dokunma titreşimi",
                subtitle = "Tuşlara basınca kısa titreşim",
                checked = haptics,
                onCheckedChange = { haptics = it },
            )

            Spacer(Modifier.height(22.dp))

            Button(
                onClick = { onSave(host, token, keepAwake, haptics) },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(52.dp),
                shape = RoundedCornerShape(18.dp),
                colors = ButtonDefaults.buttonColors(
                    containerColor = Blue,
                    contentColor = TextPrimary,
                ),
            ) {
                Text("Kaydet ve bağlan", style = MaterialTheme.typography.labelLarge)
            }

            Spacer(Modifier.height(10.dp))

            OutlinedButton(
                onClick = onReconnect,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(50.dp),
                shape = RoundedCornerShape(18.dp),
                colors = ButtonDefaults.outlinedButtonColors(contentColor = TextPrimary),
            ) {
                Text("Yeniden bağlan", style = MaterialTheme.typography.labelLarge)
            }

            Spacer(Modifier.height(16.dp))

            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Sunucu", style = MaterialTheme.typography.bodyMedium, color = TextMuted)
                    Text(
                        when (ui.connection) {
                            is Connection.Online -> "Bağlı"
                            is Connection.Connecting -> "Bağlanıyor…"
                            is Connection.Offline -> "Çevrimdışı"
                        },
                        style = MaterialTheme.typography.bodyLarge,
                        color = TextPrimary,
                    )
                }
                if (ui.serverVersion.isNotBlank()) {
                    Text(
                        "v${ui.serverVersion.removePrefix("v")}",
                        style = MaterialTheme.typography.labelSmall,
                        color = Cyan,
                    )
                }
            }

            Spacer(Modifier.height(14.dp))

            TextButton(onClick = onDisconnect) {
                Text("Bağlantıyı kes", color = Red)
            }

            Spacer(Modifier.height(4.dp))

            Text(
                "Pano düzenleme, ses mikseri ve geçmiş grafikleri PC'deki web arayüzünde.",
                style = MaterialTheme.typography.bodyMedium,
                color = TextMuted,
            )
        }
    }
}

@Composable
private fun SwitchRow(
    title: String,
    subtitle: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(Card, RoundedCornerShape(16.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge, color = TextPrimary)
            Text(subtitle, style = MaterialTheme.typography.bodyMedium, color = TextMuted)
        }
        Switch(
            checked = checked,
            onCheckedChange = onCheckedChange,
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

@Composable
private fun settingsFieldColors() = OutlinedTextFieldDefaults.colors(
    focusedTextColor = TextPrimary,
    unfocusedTextColor = TextPrimary,
    focusedBorderColor = Blue,
    unfocusedBorderColor = Stroke,
    focusedLabelColor = Blue,
    unfocusedLabelColor = TextMuted,
    cursorColor = Blue,
    focusedContainerColor = Card,
    unfocusedContainerColor = Card,
    focusedPlaceholderColor = TextMuted.copy(alpha = 0.6f),
    unfocusedPlaceholderColor = TextMuted.copy(alpha = 0.6f),
)
