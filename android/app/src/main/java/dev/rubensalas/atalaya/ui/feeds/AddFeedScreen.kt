package dev.rubensalas.atalaya.ui.feeds

import android.widget.Toast
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.RadioButtonDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.DiscoveredFeed
import dev.rubensalas.atalaya.ui.theme.Atalaya

/** Añadir un feed (pantalla 10, F1 a F3). */
@Composable
fun AddFeedScreen(container: AppContainer, onDone: () -> Unit) {
    val vm = viewModel { AddFeedViewModel(container.client) }
    val state by vm.state.collectAsStateWithLifecycle()
    val focus = LocalFocusManager.current
    var newFolder by remember { mutableStateOf(false) }

    val context = LocalContext.current
    LaunchedEffect(state.done) {
        if (!state.done) return@LaunchedEffect
        if (state.slow) {
            Toast.makeText(
                context,
                "El feed se está creando: Miniflux descarga sus artículos. Aparecerá en unos minutos.",
                Toast.LENGTH_LONG,
            ).show()
        }
        onDone()
    }

    Scaffold(
        topBar = {
            FeedsTopBar(
                title = "Añadir feed",
                navigationIcon = {
                    IconButton(onClick = onDone) { Icon(Icons.Filled.Close, contentDescription = "Cerrar") }
                },
            )
        },
        containerColor = Atalaya.colors.bg,
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .consumeWindowInsets(padding)
                .imePadding()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Column {
                FieldLabel("Dirección de la web o del feed")
                FormTextField(
                    value = state.url,
                    onValueChange = vm::setUrl,
                    enabled = !state.adding,
                    trailingIcon = if (state.url.isNotEmpty()) {
                        { IconButton(onClick = { vm.setUrl("") }) { Icon(Icons.Filled.Close, "Borrar", tint = Atalaya.colors.text3) } }
                    } else {
                        null
                    },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Search),
                    keyboardActions = KeyboardActions(onSearch = {
                        focus.clearFocus()
                        vm.discover()
                    }),
                )
                HelpText("Basta con la dirección de la web: Atalaya busca sus feeds.")
            }

            val found = state.found
            when {
                state.searching -> Row(verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator(Modifier.padding(end = 12.dp).size(20.dp), color = Atalaya.colors.text3, strokeWidth = 2.dp)
                    Text("Buscando feeds…", fontSize = 14.sp, color = Atalaya.colors.text2)
                }
                found == null -> SecondaryButton(
                    "Buscar feeds",
                    onClick = {
                        focus.clearFocus()
                        vm.discover()
                    },
                    enabled = state.url.isNotBlank(),
                )
                found.isNotEmpty() -> Column {
                    FieldLabel(if (found.size == 1) "Feed encontrado" else "Feeds encontrados")
                    FoundFeeds(found, state.selected, vm::select)
                }
            }

            state.error?.let { ErrorText(it) }

            if (!found.isNullOrEmpty()) {
                Column {
                    FieldLabel("Carpeta")
                    FolderPicker(
                        categories = state.categories,
                        selected = state.categoryId,
                        onSelect = vm::setCategory,
                        enabled = !state.adding,
                        onNewFolder = { newFolder = true },
                    )
                }
                CrawlerToggle(state.crawler, vm::setCrawler, enabled = !state.adding)
                if (state.crawler) {
                    ScraperRulesField(state.scraperRules, vm::setScraperRules, enabled = !state.adding)
                }
                PrimaryButton("Añadir", onClick = vm::add, enabled = state.canAdd, busy = state.adding)
            }
        }
    }

    if (newFolder) {
        NewFolderDialog(
            onDismiss = { newFolder = false },
            onCreate = {
                vm.createFolder(it)
                newFolder = false
            },
        )
    }
}

/** `.found`: un recuadro con un botón de radio por feed, con idioma y frecuencia (F2). */
@Composable
private fun FoundFeeds(found: List<DiscoveredFeed>, selected: Int, onSelect: (Int) -> Unit) {
    val shape = RoundedCornerShape(12.dp)
    Column(Modifier.fillMaxWidth().border(1.dp, Atalaya.colors.divider, shape)) {
        found.forEachIndexed { i, feed ->
            if (i > 0) HorizontalDivider(color = Atalaya.colors.divider)
            Row(
                Modifier
                    .fillMaxWidth()
                    .selectable(selected = i == selected, role = Role.RadioButton) { onSelect(i) }
                    .padding(horizontal = 14.dp, vertical = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                RadioButton(
                    selected = i == selected,
                    onClick = null,
                    colors = RadioButtonDefaults.colors(selectedColor = Atalaya.colors.accent, unselectedColor = Atalaya.colors.text3),
                )
                Column(Modifier.weight(1f)) {
                    Text(feed.title.ifBlank { withoutScheme(feed.feedUrl) }, fontSize = 14.5.sp, color = Atalaya.colors.text)
                    Text(
                        languageAndRate(feed.lang, feed.articlesPerDay),
                        fontSize = 12.5.sp,
                        lineHeight = 17.sp,
                        color = Atalaya.colors.text3,
                        modifier = Modifier.padding(top = 3.dp),
                    )
                    if (found.size > 1) {
                        Text(
                            withoutScheme(feed.feedUrl),
                            fontSize = 12.5.sp,
                            color = Atalaya.colors.text3,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun NewFolderDialog(onDismiss: () -> Unit, onCreate: (String) -> Unit) {
    var title by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Nueva carpeta") },
        text = { FormTextField(value = title, onValueChange = { title = it }) },
        confirmButton = { TextButton(onClick = { onCreate(title) }, enabled = title.isNotBlank()) { Text("Crear") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancelar") } },
    )
}
