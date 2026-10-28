package dev.rubensalas.atalaya.ui.article

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Translate
import androidx.compose.material.icons.outlined.BookmarkBorder
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.Progress
import dev.rubensalas.atalaya.data.api.TranslationState
import dev.rubensalas.atalaya.ui.list.articleAge
import dev.rubensalas.atalaya.ui.theme.Atalaya

/** Artículo abierto (pantallas 3 y 4). */
@Composable
fun ArticleScreen(container: AppContainer, id: Id, onBack: () -> Unit) {
    val vm: ArticleViewModel = viewModel(
        key = "article-$id",
        factory = viewModelFactory {
            initializer { ArticleViewModel(id, container.list, container.papers, serviceUrl = { container.settings.current().serviceUrl }) }
        },
    )
    val state by vm.state.collectAsStateWithLifecycle()
    val full by vm.full.collectAsStateWithLifecycle()
    val c = Atalaya.colors
    val snackbar = remember { SnackbarHostState() }
    // Fuera de la rama del cuerpo: la posición de lectura sobrevive a cada versión nueva (ADR 0015).
    val listState = rememberLazyListState()

    LifecycleEventEffect(Lifecycle.Event.ON_START) { vm.setActive(true) }
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { vm.setActive(false) }

    LaunchedEffect(state.message) {
        state.message?.let {
            snackbar.showSnackbar(it)
            vm.dismissMessage()
        }
    }

    Scaffold(
        containerColor = c.bg,
        snackbarHost = { SnackbarHost(snackbar) },
        topBar = { ArticleTopBar(state, full, onBack, vm) },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            val entry = state.entry
            when {
                entry == null && state.error != null -> Column(
                    Modifier.fillMaxSize().padding(24.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(state.error.orEmpty(), color = c.text2, textAlign = TextAlign.Center)
                    OutlinedButton(onClick = vm::load) { Text("Reintentar") }
                }
                entry == null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = c.text2)
                }
                else -> ArticleBody(entry, state, full, vm::readFull, listState)
            }
            // Con avance, la barra de arriba ya dice que se traduce: el aviso de abajo sobra.
            if (state.translating && state.progress == null) TranslatingToast(Modifier.align(Alignment.BottomCenter).padding(bottom = 20.dp))
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ArticleTopBar(state: ArticleUiState, full: FullTextState, onBack: () -> Unit, vm: ArticleViewModel) {
    val c = Atalaya.colors
    val entry = state.entry
    Column(Modifier.background(c.bg)) {
        TopAppBar(
            navigationIcon = {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Volver")
                }
            },
            title = { if (state.showSelector) VersionSelector(state.showing, state.canSwitch, onChoose = vm::choose) },
            actions = { if (entry != null) ArticleActions(entry, vm) },
            colors = TopAppBarDefaults.topAppBarColors(containerColor = c.bg),
        )
        // Mientras se traduce, una barra fina ocupa el sitio de la línea (pantalla 4).
        // El avance del texto completo de un paper gana al del artículo, como en el paper (#90).
        TopBarProgress(progress = full.progress ?: state.progress, translating = state.translating)
    }
}

/**
 * La línea bajo la cabecera: con avance, la barra con «Traduciendo… N de M»; si solo se
 * traduce, una barra fina; si no, el divisor (pantallas 3, 4 y 8).
 */
@Composable
internal fun TopBarProgress(progress: Progress?, translating: Boolean) {
    val c = Atalaya.colors
    when {
        progress != null -> TranslationProgress(progress)
        translating -> LinearProgressIndicator(
            modifier = Modifier.fillMaxWidth().height(2.dp),
            color = c.accent,
            trackColor = c.surface2,
            gapSize = 0.dp,
        )
        else -> HorizontalDivider(thickness = 1.dp, color = c.divider)
    }
}

/** La barra fina con «Traduciendo… N de M», bajo la cabecera (ADR 0015). Su alto es fijo: no mueve el texto al refrescar. */
@Composable
internal fun TranslationProgress(progress: Progress) {
    val c = Atalaya.colors
    Column(Modifier.fillMaxWidth().height(PROGRESS_STRIP_HEIGHT)) {
        LinearProgressIndicator(
            progress = { translationFraction(progress) },
            modifier = Modifier.fillMaxWidth().height(2.dp),
            color = c.accent,
            trackColor = c.surface2,
            gapSize = 0.dp,
        )
        Text(
            translationProgressText(progress),
            fontSize = 12.5.sp,
            color = c.text2,
            modifier = Modifier.padding(start = 20.dp, top = 4.dp),
        )
    }
}

private val PROGRESS_STRIP_HEIGHT = 26.dp

@Composable
private fun ArticleActions(entry: Entry, vm: ArticleViewModel) {
    var menu by remember { mutableStateOf(false) }
    val context = LocalContext.current
    IconButton(onClick = vm::toggleStarred) {
        if (entry.starred) {
            Icon(Icons.Filled.Bookmark, contentDescription = "Quitar de guardados")
        } else {
            Icon(Icons.Outlined.BookmarkBorder, contentDescription = "Guardar")
        }
    }
    Box {
        IconButton(onClick = { menu = true }) {
            Icon(Icons.Filled.MoreVert, contentDescription = "Más opciones")
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(
                text = { Text(if (entry.read) "Marcar como no leído" else "Marcar como leído") },
                onClick = {
                    menu = false
                    vm.toggleRead()
                },
            )
            DropdownMenuItem(
                text = { Text("Volver a traducir") },
                onClick = {
                    menu = false
                    vm.retranslate()
                },
            )
            DropdownMenuItem(
                text = { Text("Compartir el enlace") },
                onClick = {
                    menu = false
                    val send = Intent(Intent.ACTION_SEND).apply {
                        type = "text/plain"
                        putExtra(Intent.EXTRA_SUBJECT, entry.title)
                        putExtra(Intent.EXTRA_TEXT, entry.url)
                    }
                    context.startActivity(Intent.createChooser(send, null))
                },
            )
        }
    }
}

/** «Español | Original». Apagado hasta que hay traducción (pantalla 4). También lo usa el paper abierto. */
@Composable
internal fun VersionSelector(showing: ArticleVersion, canSwitch: Boolean, onChoose: (ArticleVersion) -> Unit) {
    val c = Atalaya.colors
    Row(
        Modifier
            .padding(start = 4.dp)
            .alpha(if (canSwitch) 1f else 0.45f)
            .background(c.surface, RoundedCornerShape(18.dp))
            .padding(3.dp),
    ) {
        for ((version, label) in listOf(ArticleVersion.Spanish to "Español", ArticleVersion.Original to "Original")) {
            val on = showing == version
            Text(
                label,
                fontSize = 13.5.sp,
                fontWeight = if (on) FontWeight.Medium else FontWeight.Normal,
                color = if (on) c.onAccent else c.text2,
                modifier = Modifier
                    .background(if (on) c.accent else c.surface, RoundedCornerShape(15.dp))
                    .semantics { selected = on }
                    .clickable(enabled = canSwitch, role = Role.Tab) { onChoose(version) }
                    .padding(horizontal = 12.dp, vertical = 6.dp),
            )
        }
    }
}

@Composable
private fun ArticleBody(entry: Entry, state: ArticleUiState, full: FullTextState, onReadFull: () -> Unit, listState: LazyListState) {
    val c = Atalaya.colors
    val uri = LocalUriHandler.current
    val muted = state.translating && state.showing == ArticleVersion.Original
    LazyColumn(
        Modifier.fillMaxSize(),
        state = listState,
        contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 20.dp, bottom = 24.dp),
    ) {
        item(key = "head") {
            Column {
                Text(
                    listOfNotNull(entry.feedTitle, entry.author, articleAge(entry.publishedAt)).joinToString(" · "),
                    fontSize = 13.sp,
                    color = c.text2,
                    modifier = Modifier.padding(bottom = 10.dp),
                )
                Text(
                    state.title,
                    fontSize = 24.sp,
                    lineHeight = 31.sp,
                    fontWeight = FontWeight.Medium,
                    color = c.text,
                    modifier = Modifier.padding(bottom = 12.dp),
                )
                Note(entry, state)
            }
        }
        // La clave es solo la posición: al pasar del original al español, o al llegar más párrafos, no se recrean los bloques.
        itemsIndexed(state.blocks, key = { i, _ -> "block-$i" }) { _, block ->
            ArticleBlock(block, muted = muted)
        }
        item(key = "web") {
            Row(
                Modifier
                    .padding(top = 8.dp)
                    .fillMaxWidth()
                    .height(44.dp)
                    .border(1.dp, c.divider, RoundedCornerShape(22.dp))
                    .clickable(role = Role.Button) { uri.openUri(entry.url) },
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null, tint = c.text, modifier = Modifier.size(18.dp))
                Text("Abrir en la web", fontSize = 14.5.sp, color = c.text)
            }
        }
        // Solo si el artículo es un paper de arXiv (#90): «Leer completo» y el texto debajo.
        if (entry.arxivId != null) {
            item(key = "full-action") { FullTextAction(full, onReadFull, Modifier.padding(top = 8.dp)) }
            fullTextSection(full, onReadFull)
        }
    }
}

/** La etiqueta bajo el título: «Traducido» o por qué no lo está. */
@Composable
private fun Note(entry: Entry, state: ArticleUiState) {
    val text = when {
        // Mientras se traduce, lo dice la barra de arriba: aquí no hay que decir «Traducido» a medias.
        state.showing == ArticleVersion.Spanish && !state.translating -> "Traducido"
        entry.translation == TranslationState.Failed -> "No se pudo traducir"
        else -> return
    }
    TranslationNote(text)
}

/** La etiqueta con el icono de traducir: «Traducido», «No se pudo traducir». */
@Composable
internal fun TranslationNote(text: String) {
    val c = Atalaya.colors
    Row(
        Modifier
            .padding(bottom = 18.dp)
            .border(1.dp, c.divider, RoundedCornerShape(14.dp))
            .padding(start = 8.dp, end = 10.dp, top = 3.dp, bottom = 3.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Filled.Translate, contentDescription = null, tint = c.text3, modifier = Modifier.size(14.dp))
        Text(text, fontSize = 12.5.sp, color = c.text3)
    }
}

@Composable
internal fun TranslatingToast(modifier: Modifier = Modifier) {
    val c = Atalaya.colors
    Row(
        modifier
            .background(c.accent, RoundedCornerShape(20.dp))
            .padding(horizontal = 16.dp, vertical = 9.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Filled.Translate, contentDescription = null, tint = c.onAccent, modifier = Modifier.size(18.dp))
        Text("Traduciendo…", fontSize = 13.5.sp, color = c.onAccent)
    }
}
