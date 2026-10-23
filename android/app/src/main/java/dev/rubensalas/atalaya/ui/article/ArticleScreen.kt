package dev.rubensalas.atalaya.ui.article

import androidx.compose.runtime.Composable
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.ui.Placeholder

/** Artículo abierto (pantallas 3 y 4). Paso 3.3. */
@Composable
fun ArticleScreen(container: AppContainer, id: Id, onBack: () -> Unit) {
    Placeholder("El artículo llega en el paso 3.3")
}
