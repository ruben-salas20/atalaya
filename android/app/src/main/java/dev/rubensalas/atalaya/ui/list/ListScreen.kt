package dev.rubensalas.atalaya.ui.list

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.material3.Surface
import androidx.compose.material3.TextButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.ui.Placeholder

/** Lista de artículos con el menú lateral (pantallas 1 y 2). Paso 3.2. */
@Composable
fun ListScreen(
    container: AppContainer,
    onOpenArticle: (Id) -> Unit,
    onOpenSettings: () -> Unit,
    onOpenFeeds: () -> Unit,
) {
    Surface(Modifier.fillMaxSize()) {
        Column(Modifier.safeDrawingPadding()) {
            TextButton(onClick = onOpenSettings) { Text("Ajustes") }
            TextButton(onClick = onOpenFeeds) { Text("Feeds") }
            Placeholder("La lista llega en el paso 3.2")
        }
    }
}
