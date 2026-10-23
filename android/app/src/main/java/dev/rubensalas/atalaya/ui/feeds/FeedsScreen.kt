package dev.rubensalas.atalaya.ui.feeds

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.RssFeed
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.Feed
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.ui.theme.Atalaya

/** Feeds por carpeta (pantalla 9, F5 y F6). */
@Composable
fun FeedsScreen(container: AppContainer, onBack: () -> Unit, onAddFeed: () -> Unit, onEditFeed: (Id) -> Unit) {
    val vm = viewModel { FeedsViewModel(container.client) }
    val state by vm.state.collectAsStateWithLifecycle()
    // Al volver de añadir o editar, la lista cambia.
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { vm.load() }

    Scaffold(
        topBar = {
            FeedsTopBar(
                title = "Feeds",
                navigationIcon = {
                    IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Volver") }
                },
                actions = {
                    IconButton(onClick = onAddFeed) { Icon(Icons.Filled.Add, contentDescription = "Añadir feed") }
                },
            )
        },
        containerColor = Atalaya.colors.bg,
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            when {
                state.loading -> CenteredLoading()
                state.error != null && state.groups.isEmpty() -> LoadError(state.error!!, vm::load)
                state.groups.isEmpty() -> LoadError("Aún no hay feeds. Añade uno con el botón +.", vm::load)
                else -> LazyColumn(Modifier.fillMaxSize()) {
                    state.groups.forEach { group ->
                        item(key = "c${group.category.id}") {
                            FolderHeader(group.category.title) { vm.openFolder(group.category) }
                        }
                        if (group.feeds.isEmpty()) {
                            item(key = "e${group.category.id}") {
                                Text(
                                    "Carpeta vacía",
                                    fontSize = 13.sp,
                                    color = Atalaya.colors.text3,
                                    modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
                                )
                            }
                        }
                        items(group.feeds, key = { "f${it.id}" }) { feed ->
                            FeedRow(feed) { onEditFeed(feed.id) }
                        }
                    }
                }
            }
        }
    }

    state.folderDialog?.let { dialog ->
        FolderEditDialog(
            dialog = dialog,
            onDismiss = vm::closeFolder,
            onRename = vm::renameFolder,
            onDelete = vm::deleteFolder,
        )
    }
}

/** `.fd-folder`: nombre en mayúsculas y lápiz para renombrar o borrar. */
@Composable
private fun FolderHeader(title: String, onEdit: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(start = 20.dp, end = 8.dp, top = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            title.uppercase(),
            fontSize = 12.5.sp,
            letterSpacing = 0.5.sp,
            color = Atalaya.colors.text3,
            modifier = Modifier.weight(1f),
        )
        IconButton(onClick = onEdit) {
            Icon(
                Icons.Filled.Edit,
                contentDescription = "Editar la carpeta $title",
                tint = Atalaya.colors.text3,
                modifier = Modifier.size(18.dp),
            )
        }
    }
}

/** `.fd`: un feed con su web, o su fallo si no se pudo actualizar (F6). */
@Composable
private fun FeedRow(feed: Feed, onClick: () -> Unit) {
    val subtitle = feedSubtitle(feed)
    Column {
        Row(
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = 20.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Icon(Icons.Filled.RssFeed, contentDescription = null, tint = Atalaya.colors.text3, modifier = Modifier.size(18.dp))
            Column(Modifier.weight(1f)) {
                Text(feed.title, fontSize = 15.sp, color = Atalaya.colors.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Row(Modifier.padding(top = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                    if (subtitle.failing) {
                        Icon(
                            Icons.Filled.Warning,
                            contentDescription = null,
                            tint = Atalaya.colors.text,
                            modifier = Modifier.padding(end = 5.dp).size(14.dp),
                        )
                    }
                    Text(
                        subtitle.text,
                        fontSize = 12.5.sp,
                        color = if (subtitle.failing) Atalaya.colors.text else Atalaya.colors.text3,
                        fontWeight = if (subtitle.failing) FontWeight.Medium else FontWeight.Normal,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
            Icon(
                Icons.AutoMirrored.Filled.KeyboardArrowRight,
                contentDescription = null,
                tint = Atalaya.colors.text3,
                modifier = Modifier.size(18.dp),
            )
        }
        HorizontalDivider(color = Atalaya.colors.divider)
    }
}

/** Renombrar o borrar una carpeta (F5). Si tiene feeds, el servicio no la borra y se muestra su mensaje. */
@Composable
private fun FolderEditDialog(
    dialog: FolderDialog,
    onDismiss: () -> Unit,
    onRename: (String) -> Unit,
    onDelete: () -> Unit,
) {
    var title by remember(dialog.category.id) { mutableStateOf(dialog.category.title) }
    AlertDialog(
        onDismissRequest = { if (!dialog.busy) onDismiss() },
        title = { Text("Carpeta") },
        text = {
            Column {
                FieldLabel("Nombre")
                FormTextField(value = title, onValueChange = { title = it }, enabled = !dialog.busy)
                dialog.error?.let { ErrorText(it, Modifier.padding(top = 12.dp)) }
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onRename(title) },
                enabled = !dialog.busy && title.isNotBlank() && title.trim() != dialog.category.title,
            ) { Text("Guardar") }
        },
        dismissButton = {
            Row {
                TextButton(onClick = onDelete, enabled = !dialog.busy) { Text("Borrar carpeta") }
                TextButton(onClick = onDismiss, enabled = !dialog.busy) { Text("Cancelar") }
            }
        },
    )
}
