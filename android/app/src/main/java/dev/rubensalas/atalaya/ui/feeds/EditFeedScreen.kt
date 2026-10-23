package dev.rubensalas.atalaya.ui.feeds

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.ui.relativeTime
import dev.rubensalas.atalaya.ui.theme.Atalaya

/** Editar un feed (pantalla 11, F4 y F6). */
@Composable
fun EditFeedScreen(container: AppContainer, id: Id, onDone: () -> Unit) {
    val vm = viewModel { EditFeedViewModel(container.client, id) }
    val state by vm.state.collectAsStateWithLifecycle()

    LaunchedEffect(state.done) { if (state.done) onDone() }

    Scaffold(
        topBar = {
            FeedsTopBar(
                title = state.feed?.title ?: "Feed",
                navigationIcon = {
                    IconButton(onClick = onDone) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Volver") }
                },
            )
        },
        containerColor = Atalaya.colors.bg,
    ) { padding ->
        val feed = state.feed
        Column(Modifier.fillMaxSize().padding(padding)) {
            when {
                state.loading -> CenteredLoading()
                feed == null -> LoadError(state.error ?: "No se pudo cargar el feed", vm::load)
                else -> Column(
                    Modifier
                        .fillMaxSize()
                        .consumeWindowInsets(padding)
                        .imePadding()
                        .verticalScroll(rememberScrollState())
                        .padding(horizontal = 20.dp, vertical = 16.dp),
                    verticalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    Column {
                        FieldLabel("Nombre")
                        FormTextField(value = state.title, onValueChange = vm::setTitle, enabled = !state.saving)
                    }
                    Column {
                        FieldLabel("Carpeta")
                        FolderPicker(state.categories, state.categoryId, vm::setCategory, enabled = !state.saving)
                    }
                    CrawlerToggle(state.crawler, vm::setCrawler, enabled = !state.saving)
                    Column {
                        StatRow("Dirección", withoutScheme(feed.feedUrl))
                        StatRow("Última actualización", feed.checkedAt?.let { relativeTime(it) } ?: "Nunca")
                        StatRow("Idioma", languageName(feed.lang) ?: "Sin detectar")
                        perDayText(feed.articlesPerDay)?.let { StatRow("Frecuencia", it.replaceFirstChar(Char::uppercase)) }
                        feed.error?.let {
                            val times = if (it.count == 1) "1 vez" else "${it.count} veces"
                            StatRow("Fallo al actualizar ($times)", it.message)
                        }
                    }
                    state.error?.let { ErrorText(it) }
                    PrimaryButton("Guardar cambios", onClick = vm::save, enabled = state.canSave, busy = state.saving)
                    SecondaryButton("Quitar feed", onClick = { vm.askDelete(true) }, enabled = !state.saving)
                }
            }
        }
    }

    if (state.confirmDelete) {
        AlertDialog(
            onDismissRequest = { vm.askDelete(false) },
            title = { Text("¿Quitar ${state.feed?.title ?: "el feed"}?") },
            text = { Text("Se borran también sus artículos. Los guardados se pierden.") },
            confirmButton = { TextButton(onClick = vm::delete) { Text("Quitar") } },
            dismissButton = { TextButton(onClick = { vm.askDelete(false) }) { Text("Cancelar") } },
        )
    }
}
