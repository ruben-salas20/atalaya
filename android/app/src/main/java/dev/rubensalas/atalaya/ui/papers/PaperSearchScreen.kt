package dev.rubensalas.atalaya.ui.papers

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Translate
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.PaperField
import dev.rubensalas.atalaya.data.api.PaperSort
import dev.rubensalas.atalaya.data.api.PaperSummary
import dev.rubensalas.atalaya.data.papers.RecentSearch
import dev.rubensalas.atalaya.ui.theme.Atalaya

/** Buscar papers (pantallas 6 y 7, P1–P3 y P6). */
@Composable
fun PaperSearchScreen(container: AppContainer, onBack: () -> Unit, onOpenPaper: (String) -> Unit) {
    val vm: PaperSearchViewModel = viewModel(
        factory = viewModelFactory { initializer { PaperSearchViewModel(container.papers, container.recentSearches) } },
    )
    val state by vm.state.collectAsStateWithLifecycle()
    val c = Atalaya.colors
    val focus = LocalFocusManager.current
    val focusRequester = remember { FocusRequester() }

    // Al abrir sin nada buscado, el teclado sale ya.
    LaunchedEffect(Unit) { if (state.submitted == null) focusRequester.requestFocus() }

    val search = {
        focus.clearFocus()
        vm.search()
    }

    Scaffold(
        containerColor = c.bg,
        topBar = {
            Row(
                Modifier
                    .background(c.bg)
                    .statusBarsPadding()
                    .fillMaxWidth()
                    .height(64.dp)
                    .padding(end = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Volver")
                }
                SearchField(
                    text = state.text,
                    onTextChange = vm::setText,
                    onSearch = search,
                    onFocus = vm::startEditing,
                    focusRequester = focusRequester,
                    modifier = Modifier.weight(1f),
                )
            }
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding).imePadding()) {
            if (state.showResults) {
                Results(state, onOpenPaper = onOpenPaper, onLoadMore = vm::loadMore, onRetry = vm::search)
            } else {
                SearchOptions(
                    state,
                    onField = vm::setField,
                    onSort = vm::setSort,
                    onRecent = {
                        focus.clearFocus()
                        vm.searchRecent(it)
                    },
                    onRemoveRecent = vm::removeRecent,
                )
            }
        }
    }
}

@Composable
private fun SearchField(
    text: String,
    onTextChange: (String) -> Unit,
    onSearch: () -> Unit,
    onFocus: () -> Unit,
    focusRequester: FocusRequester,
    modifier: Modifier = Modifier,
) {
    val c = Atalaya.colors
    Row(
        modifier
            .height(42.dp)
            .background(c.surface, RoundedCornerShape(21.dp))
            .padding(start = 14.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Filled.Search, contentDescription = null, tint = c.text3, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(10.dp))
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
            if (text.isEmpty()) Text("Buscar papers", fontSize = 15.sp, color = c.text3)
            BasicTextField(
                value = text,
                onValueChange = onTextChange,
                singleLine = true,
                textStyle = TextStyle(fontSize = 15.sp, color = c.text),
                cursorBrush = SolidColor(c.accent),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { onSearch() }),
                modifier = Modifier
                    .fillMaxWidth()
                    .focusRequester(focusRequester)
                    .onFocusChanged { if (it.isFocused) onFocus() },
            )
        }
        if (text.isNotEmpty()) {
            IconButton(onClick = { onTextChange("") }, modifier = Modifier.size(36.dp)) {
                Icon(Icons.Filled.Close, contentDescription = "Borrar", tint = c.text3, modifier = Modifier.size(18.dp))
            }
        }
    }
}

/** Pantalla 6: cómo se buscará, los filtros y las búsquedas recientes. */
@Composable
private fun SearchOptions(
    state: PaperSearchUiState,
    onField: (PaperField) -> Unit,
    onSort: (PaperSort) -> Unit,
    onRecent: (RecentSearch) -> Unit,
    onRemoveRecent: (RecentSearch) -> Unit,
) {
    val c = Atalaya.colors
    LazyColumn(Modifier.fillMaxSize()) {
        val english = state.english
        if (english != null && state.text.isNotBlank()) {
            item(key = "english") {
                Row(
                    Modifier.padding(start = 18.dp, end = 18.dp, top = 8.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Filled.Translate, contentDescription = null, tint = c.text3, modifier = Modifier.size(14.dp))
                    Text("Se buscará en inglés: «$english»", fontSize = 12.5.sp, color = c.text3)
                }
            }
        }
        item(key = "chips") {
            Row(
                Modifier.padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                DropChip("Buscar en: ${state.field.label()}", PaperField.entries, PaperField::label, onField)
                DropChip(state.sort.label(), PaperSort.entries, PaperSort::label, onSort)
            }
        }
        if (state.recents.isNotEmpty()) {
            item(key = "recent-h") {
                Text(
                    "BÚSQUEDAS RECIENTES",
                    fontSize = 12.5.sp,
                    letterSpacing = 0.5.sp,
                    color = c.text3,
                    modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 18.dp, bottom = 4.dp),
                )
            }
            items(state.recents, key = { "recent-${it.field}-${it.q}" }) { recent ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .clickable { onRecent(recent) }
                        .padding(start = 20.dp, end = 8.dp)
                        .height(46.dp),
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Filled.History, contentDescription = null, tint = c.text3, modifier = Modifier.size(20.dp))
                    Text(
                        recent.label(),
                        fontSize = 15.sp,
                        color = c.text,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    IconButton(onClick = { onRemoveRecent(recent) }) {
                        Icon(Icons.Filled.Close, contentDescription = "Quitar de recientes", tint = c.text3, modifier = Modifier.size(20.dp))
                    }
                }
            }
        }
    }
}

/** Un desplegable con forma de etiqueta: «Buscar en: Todo ⌄» (P2). */
@Composable
private fun <T> DropChip(label: String, options: List<T>, optionLabel: (T) -> String, onSelect: (T) -> Unit) {
    val c = Atalaya.colors
    var open by remember { mutableStateOf(false) }
    Box {
        Row(
            Modifier
                .clip(RoundedCornerShape(15.dp))
                .border(1.dp, c.divider, RoundedCornerShape(15.dp))
                .clickable { open = true }
                .padding(start = 11.dp, end = 8.dp, top = 5.dp, bottom = 5.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(label, fontSize = 13.sp, color = c.text)
            Icon(Icons.Filled.ExpandMore, contentDescription = null, tint = c.text2, modifier = Modifier.size(16.dp))
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            for (option in options) {
                DropdownMenuItem(
                    text = { Text(optionLabel(option)) },
                    onClick = {
                        open = false
                        onSelect(option)
                    },
                )
            }
        }
    }
}

/** Pantalla 7: los resultados, con la cabecera «38 resultados · buscado como …». */
@Composable
private fun Results(
    state: PaperSearchUiState,
    onOpenPaper: (String) -> Unit,
    onLoadMore: () -> Unit,
    onRetry: () -> Unit,
) {
    val c = Atalaya.colors
    when {
        state.searching -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            CircularProgressIndicator(color = c.text2)
        }
        state.error != null && state.papers.isEmpty() -> Column(
            Modifier.fillMaxSize().padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(state.error, color = c.text2, textAlign = TextAlign.Center)
            OutlinedButton(onClick = onRetry) { Text("Reintentar") }
        }
        state.papers.isEmpty() -> Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
            Text(
                "No hay papers para «${state.englishQuery.ifEmpty { state.submitted?.q.orEmpty() }}»",
                color = c.text2,
                textAlign = TextAlign.Center,
            )
        }
        else -> LazyColumn(Modifier.fillMaxSize()) {
            item(key = "head") {
                Column {
                    Text(
                        resultsHeader(state.total, state.englishQuery),
                        fontSize = 12.5.sp,
                        lineHeight = 17.sp,
                        color = c.text3,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                    )
                    HorizontalDivider(thickness = 1.dp, color = c.divider)
                }
            }
            items(state.papers, key = { it.arxivId }) { paper ->
                PaperRow(paper, onClick = { onOpenPaper(paper.arxivId) })
            }
            if (state.canLoadMore || state.loadingMore) {
                item(key = "more") {
                    LaunchedEffect(state.papers.size) { onLoadMore() }
                    Box(Modifier.fillMaxWidth().padding(16.dp), contentAlignment = Alignment.Center) {
                        CircularProgressIndicator(Modifier.size(24.dp), color = c.text3, strokeWidth = 2.dp)
                    }
                }
            } else if (state.error != null) {
                item(key = "more-error") {
                    Text(
                        state.error,
                        fontSize = 13.sp,
                        color = c.text2,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.fillMaxWidth().padding(16.dp),
                    )
                }
            }
        }
    }
}

/** Una fila de resultados (P3): título traducido, autores, fecha, marcador si está guardado y categoría. */
@Composable
private fun PaperRow(paper: PaperSummary, onClick: () -> Unit) {
    val c = Atalaya.colors
    Column(Modifier.fillMaxWidth().clickable(onClick = onClick)) {
        Column(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                paper.title,
                fontSize = 15.5.sp,
                lineHeight = 21.sp,
                fontWeight = FontWeight.Medium,
                color = c.text,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Row(
                    Modifier.weight(1f),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(
                        shortAuthors(paper.authors),
                        fontSize = 13.sp,
                        color = c.text2,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    Text("· ${paperDate(paper.publishedAt)}", fontSize = 13.sp, color = c.text2, maxLines = 1)
                    if (paper.savedEntryId != null) {
                        Icon(Icons.Filled.Bookmark, contentDescription = "Guardado", tint = c.text2, modifier = Modifier.size(14.dp))
                    }
                }
                if (paper.category.isNotBlank()) {
                    Text(
                        paper.category.uppercase(),
                        fontSize = 10.5.sp,
                        lineHeight = 15.sp,
                        fontWeight = FontWeight.Medium,
                        letterSpacing = 0.5.sp,
                        color = c.text3,
                        modifier = Modifier
                            .border(1.dp, c.text3, RoundedCornerShape(4.dp))
                            .padding(horizontal = 4.dp),
                    )
                }
            }
        }
        HorizontalDivider(thickness = 1.dp, color = c.divider)
    }
}
