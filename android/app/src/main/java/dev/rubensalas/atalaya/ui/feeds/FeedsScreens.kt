package dev.rubensalas.atalaya.ui.feeds

import androidx.compose.runtime.Composable
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.ui.Placeholder

/** Feeds por carpeta (pantalla 9). Paso 3.4. */
@Composable
fun FeedsScreen(container: AppContainer, onBack: () -> Unit, onAddFeed: () -> Unit, onEditFeed: (Id) -> Unit) {
    Placeholder("Los feeds llegan en el paso 3.4")
}

/** Añadir un feed (pantalla 10). Paso 3.4. */
@Composable
fun AddFeedScreen(container: AppContainer, onDone: () -> Unit) {
    Placeholder("Añadir un feed llega en el paso 3.4")
}

/** Editar un feed (pantalla 11). Paso 3.4. */
@Composable
fun EditFeedScreen(container: AppContainer, id: Id, onDone: () -> Unit) {
    Placeholder("Editar un feed llega en el paso 3.4")
}
