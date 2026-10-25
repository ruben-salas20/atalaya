package dev.rubensalas.atalaya

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.rubensalas.atalaya.data.settings.Settings
import dev.rubensalas.atalaya.data.sync.Sync
import dev.rubensalas.atalaya.ui.navigation.AtalayaNavHost
import dev.rubensalas.atalaya.ui.theme.AtalayaTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val container = (application as AtalayaApp).container
        if (savedInstanceState == null) {
            Sync.syncNow(this)
            container.updates.checkOnOpen()
        }
        setContent {
            val settings by container.settings.settings.collectAsStateWithLifecycle(initialValue = Settings())
            AtalayaTheme(fontScale = settings.fontScale) {
                AtalayaNavHost(container)
            }
        }
    }
}
