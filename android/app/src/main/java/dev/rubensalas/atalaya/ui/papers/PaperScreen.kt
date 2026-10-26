package dev.rubensalas.atalaya.ui.papers

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.PictureAsPdf
import androidx.compose.material.icons.outlined.BookmarkBorder
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
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
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.Role
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
import dev.rubensalas.atalaya.data.api.FullPaperState
import dev.rubensalas.atalaya.data.api.Paper
import dev.rubensalas.atalaya.data.api.TranslationState
import dev.rubensalas.atalaya.ui.article.ArticleBlock
import dev.rubensalas.atalaya.ui.article.ArticleVersion
import dev.rubensalas.atalaya.ui.article.TranslatingToast
import dev.rubensalas.atalaya.ui.article.TranslationNote
import dev.rubensalas.atalaya.ui.article.TranslationProgress
import dev.rubensalas.atalaya.ui.article.VersionSelector
import dev.rubensalas.atalaya.ui.theme.Atalaya

/** Paper abierto desde la búsqueda (pantalla 8, P4). */
@Composable
fun PaperScreen(container: AppContainer, arxivId: String, onBack: () -> Unit) {
    val vm: PaperViewModel = viewModel(
        key = "paper-$arxivId",
        factory = viewModelFactory {
            initializer { PaperViewModel(arxivId, container.papers, serviceUrl = { container.settings.current().serviceUrl }) }
        },
    )
    val state by vm.state.collectAsStateWithLifecycle()
    val c = Atalaya.colors
    val snackbar = remember { SnackbarHostState() }

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
        topBar = { PaperTopBar(state, onBack, vm) },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            val paper = state.paper
            when {
                paper == null && state.error != null -> Column(
                    Modifier.fillMaxSize().padding(24.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(state.error.orEmpty(), color = c.text2, textAlign = TextAlign.Center)
                    OutlinedButton(onClick = vm::load) { Text("Reintentar") }
                }
                paper == null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = c.text2)
                }
                else -> PaperBody(paper, state, onSave = vm::save, onReadFull = vm::readFull)
            }
            if (state.translating) TranslatingToast(Modifier.align(Alignment.BottomCenter).padding(bottom = 20.dp))
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PaperTopBar(state: PaperUiState, onBack: () -> Unit, vm: PaperViewModel) {
    val c = Atalaya.colors
    val paper = state.paper
    Column(Modifier.background(c.bg)) {
        TopAppBar(
            navigationIcon = {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Volver")
                }
            },
            title = { if (state.showSelector) VersionSelector(state.showing, state.canSwitch, onChoose = vm::choose) },
            actions = { if (paper != null) PaperActions(paper, state, vm) },
            colors = TopAppBarDefaults.topAppBarColors(containerColor = c.bg),
        )
        // El avance del paper completo tiene prioridad: dice cuántos párrafos van (ADR 0015).
        val fullProgress = state.fullProgress
        when {
            fullProgress != null -> TranslationProgress(fullProgress)
            state.translating -> LinearProgressIndicator(
                modifier = Modifier.fillMaxWidth().height(2.dp),
                color = c.accent,
                trackColor = c.surface2,
                gapSize = 0.dp,
            )
            else -> HorizontalDivider(thickness = 1.dp, color = c.divider)
        }
    }
}

@Composable
private fun PaperActions(paper: Paper, state: PaperUiState, vm: PaperViewModel) {
    var menu by remember { mutableStateOf(false) }
    val context = LocalContext.current
    IconButton(onClick = vm::save, enabled = !state.saved && !state.saving) {
        if (state.saved) {
            Icon(Icons.Filled.Bookmark, contentDescription = "Guardado", tint = Atalaya.colors.text)
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
                text = { Text("Compartir el enlace") },
                onClick = {
                    menu = false
                    val send = Intent(Intent.ACTION_SEND).apply {
                        type = "text/plain"
                        putExtra(Intent.EXTRA_SUBJECT, paper.title)
                        putExtra(Intent.EXTRA_TEXT, paper.absUrl)
                    }
                    context.startActivity(Intent.createChooser(send, null))
                },
            )
        }
    }
}

@Composable
private fun PaperBody(paper: Paper, state: PaperUiState, onSave: () -> Unit, onReadFull: () -> Unit) {
    val c = Atalaya.colors
    val uri = LocalUriHandler.current
    val muted = state.translating && state.showing == ArticleVersion.Original
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 20.dp, bottom = 24.dp),
    ) {
        item(key = "head") {
            Column {
                Text(
                    "arXiv:${paper.arxivId} · ${paper.category} · ${paperDate(paper.publishedAt)}",
                    fontSize = 13.sp,
                    color = c.text2,
                    modifier = Modifier.padding(bottom = 10.dp),
                )
                Text(
                    state.title,
                    fontSize = 22.sp,
                    lineHeight = 29.sp,
                    fontWeight = FontWeight.Medium,
                    color = c.text,
                    modifier = Modifier.padding(bottom = 12.dp),
                )
                if (paper.authors.isNotEmpty()) {
                    Text(
                        paper.authors.joinToString(", "),
                        fontSize = 14.sp,
                        lineHeight = 20.sp,
                        color = c.text2,
                        modifier = Modifier.padding(bottom = 12.dp),
                    )
                }
                when {
                    state.showing == ArticleVersion.Spanish -> TranslationNote("Traducido")
                    paper.translation == TranslationState.Failed -> TranslationNote("No se pudo traducir")
                }
                Text(
                    "RESUMEN",
                    fontSize = 12.sp,
                    letterSpacing = 0.6.sp,
                    color = c.text3,
                    modifier = Modifier.padding(top = 4.dp, bottom = 8.dp),
                )
            }
        }
        itemsIndexed(state.paragraphs, key = { i, _ -> "${state.showing}-$i" }) { _, text ->
            Text(
                text,
                fontSize = 17.sp,
                lineHeight = 27.sp,
                color = if (muted) c.text3 else c.text,
                modifier = Modifier.padding(bottom = 14.dp),
            )
        }
        item(key = "actions") {
            FlowRow(
                Modifier.padding(top = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(10.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                SaveButton(saved = state.saved, busy = state.saving, onClick = onSave)
                OutlineAction("arXiv", Icons.AutoMirrored.Filled.OpenInNew) { uri.openUri(paper.absUrl) }
                OutlineAction("PDF", Icons.Filled.PictureAsPdf) { uri.openUri(paper.pdfUrl) }
                if (state.fullState == FullPaperState.NotRequested) {
                    OutlineAction("Leer completo", Icons.AutoMirrored.Filled.MenuBook, onClick = onReadFull)
                }
            }
        }
        // El paper completo va debajo del resumen, según el estado que dio el servicio (P7, ADR 0010).
        when (state.fullState) {
            FullPaperState.Pending, FullPaperState.Done -> {
                item(key = "full-head") {
                    Text(
                        "TEXTO COMPLETO",
                        fontSize = 12.sp,
                        letterSpacing = 0.6.sp,
                        color = c.text3,
                        modifier = Modifier.padding(top = 28.dp, bottom = 14.dp),
                    )
                }
                // La clave es la posición: al llegar más trozos, los que ya se leen no se recrean ni saltan.
                itemsIndexed(state.fullBlocks, key = { i, _ -> "full-$i" }) { _, block ->
                    ArticleBlock(block, muted = false)
                }
            }
            FullPaperState.NotAvailable -> item(key = "full-missing") {
                FullNote("arXiv no tiene una versión completa de este paper. Quedan el resumen y el PDF.", Modifier.padding(top = 24.dp))
            }
            FullPaperState.Failed -> item(key = "full-failed") {
                Column(Modifier.padding(top = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    FullNote("No se pudo traducir el paper completo.")
                    OutlinedButton(onClick = onReadFull) { Text("Reintentar") }
                }
            }
            FullPaperState.NotRequested -> Unit
        }
    }
}

@Composable
private fun FullNote(text: String, modifier: Modifier = Modifier) {
    Text(text, fontSize = 15.sp, lineHeight = 22.sp, color = Atalaya.colors.text2, modifier = modifier)
}

/** «Guardar», relleno. Una vez guardado queda como «Guardado» y no se puede volver a tocar. */
@Composable
private fun SaveButton(saved: Boolean, busy: Boolean, onClick: () -> Unit) {
    val c = Atalaya.colors
    Row(
        Modifier
            .height(42.dp)
            .alpha(if (busy) 0.6f else 1f)
            .background(if (saved) c.surface2 else c.accent, RoundedCornerShape(21.dp))
            .clip(RoundedCornerShape(21.dp))
            .clickable(enabled = !saved && !busy, role = Role.Button, onClick = onClick)
            .padding(horizontal = 20.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val tint = if (saved) c.text else c.onAccent
        Icon(
            if (saved) Icons.Filled.Bookmark else Icons.Outlined.BookmarkBorder,
            contentDescription = null,
            tint = tint,
            modifier = Modifier.size(18.dp),
        )
        Text(if (saved) "Guardado" else "Guardar", fontSize = 14.5.sp, fontWeight = FontWeight.Medium, color = tint)
    }
}

@Composable
private fun OutlineAction(label: String, icon: ImageVector, onClick: () -> Unit) {
    val c = Atalaya.colors
    Row(
        Modifier
            .height(42.dp)
            .border(1.dp, c.divider, RoundedCornerShape(21.dp))
            .clip(RoundedCornerShape(21.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, tint = c.text, modifier = Modifier.size(18.dp))
        Text(label, fontSize = 14.sp, color = c.text)
    }
}
