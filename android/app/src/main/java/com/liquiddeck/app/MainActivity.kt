package com.liquiddeck.app

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.liquiddeck.app.ui.ConnectScreen
import com.liquiddeck.app.ui.DeckViewModel
import com.liquiddeck.app.ui.MainShell
import com.liquiddeck.app.ui.SettingsSheet
import com.liquiddeck.app.ui.enterDeckFullscreen
import com.liquiddeck.app.ui.theme.Amoled
import com.liquiddeck.app.ui.theme.LiquidDeckTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        window.enterDeckFullscreen()
        setContent {
            LiquidDeckTheme {
                DeckApp()
            }
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) window.enterDeckFullscreen()
    }

    @Composable
    private fun DeckApp() {
        val vm: DeckViewModel = viewModel()
        val ui by vm.ui.collectAsStateWithLifecycle()
        val snackbar = remember { SnackbarHostState() }

        LaunchedEffect(vm) {
            vm.messages.collect { message ->
                snackbar.showSnackbar(message)
            }
        }

        LaunchedEffect(ui.keepAwake) {
            if (ui.keepAwake) {
                window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            } else {
                window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
            }
        }

        BackHandler(enabled = ui.showSettings) {
            vm.setShowSettings(false)
        }

        Scaffold(
            containerColor = Amoled,
            snackbarHost = { SnackbarHost(snackbar) },
        ) { padding ->
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Amoled)
                    .padding(padding),
            ) {
                if (!ui.configured) {
                    ConnectScreen(ui = ui, onConnect = vm::connect)
                } else {
                    MainShell(ui = ui, vm = vm)
                }

                if (ui.showSettings) {
                    SettingsSheet(
                        ui = ui,
                        onDismiss = { vm.setShowSettings(false) },
                        onSave = vm::applySettings,
                        onReconnect = vm::reconnect,
                        onDisconnect = vm::disconnect,
                    )
                }
            }
        }
    }
}
