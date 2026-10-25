package dev.rubensalas.atalaya.ui.list

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.DoneAll
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.View
import dev.rubensalas.atalaya.data.settings.SwipeAction
import dev.rubensalas.atalaya.ui.theme.Atalaya
import dev.rubensalas.atalaya.ui.update.UpdateBanner
import dev.rubensalas.atalaya.ui.update.startUpdate
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import java.time.Instant

/** Lista de artículos con el menú lateral (pantallas 1 y 2). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ListScreen(
    container: AppContainer,
    onOpenArticle: (Id) -> Unit,
    onOpenSettings: () -> Unit,
    onOpenFeeds: () -> Unit,
    onOpenPapers: () -> Unit,
) {
    val vm: ListViewModel = viewModel(factory = viewModelFactory { initializer { ListViewModel(container.list) } })
    val state by vm.state.collectAsStateWithLifecycle()
    val settings by container.settings.settings.collectAsStateWithLifecycle(initialValue = null)
    val update by container.updates.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val c = Atalaya.colors

    // Al entrar y al volver de un artículo, de los ajustes o de los feeds.
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) { vm.refresh() }

    // Las horas relativas («2 h», «Sincronizado (5 min)») se refrescan solas.
    var now by remember { mutableStateOf(Instant.now()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000)
            now = Instant.now()
        }
    }

    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    var expanded by rememberSaveable { mutableStateOf(setOf<Long>()) }
    val listState = rememberLazyListState()
    val snackbar = remember { SnackbarHostState() }
    val focusRequester = remember { FocusRequester() }

    BackHandler(enabled = drawerState.isOpen) { scope.launch { drawerState.close() } }
    BackHandler(enabled = state.searching) { vm.closeSearch() }

    LaunchedEffect(state.searching) { if (state.searching) focusRequester.requestFocus() }

    LaunchedEffect(state.selection) { listState.scrollToItem(0) }

    // Con artículos ya en pantalla, un fallo se avisa abajo y la lista se queda.
    LaunchedEffect(state.error, state.entries.isEmpty()) {
        val error = state.error
        if (error != null && state.entries.isNotEmpty()) {
            snackbar.showSnackbar(error)
            vm.dismissError()
        }
    }

    LaunchedEffect(listState) {
        snapshotFlow {
            val info = listState.layoutInfo
            val last = info.visibleItemsInfo.lastOrNull()?.index ?: -1
            last >= info.totalItemsCount - 10
        }.distinctUntilChanged().collect { nearEnd -> if (nearEnd) vm.loadMore() }
    }

    fun closeDrawerThen(action: () -> Unit) {
        scope.launch {
            drawerState.close()
            action()
        }
    }

    // Deslizar a la derecha sobre una fila es un gesto de la fila (L4); el menú
    // se abre con ☰ o desde la franja DrawerEdge del borde, que las filas no atienden.
    ModalNavigationDrawer(
        drawerState = drawerState,
        scrimColor = c.scrim,
        drawerContent = {
            ListDrawer(
                sidebar = state.sidebar,
                selection = state.selection,
                expanded = expanded,
                lastSyncAt = settings?.lastSyncAt,
                now = now,
                onSelect = { selection ->
                    if (selection is ListSelection.Category) expanded = expanded + selection.id
                    vm.select(selection)
                    scope.launch { drawerState.close() }
                },
                onToggleCategory = { id -> expanded = if (id in expanded) expanded - id else expanded + id },
                onOpenFeeds = { closeDrawerThen(onOpenFeeds) },
                onOpenSettings = { closeDrawerThen(onOpenSettings) },
                onOpenPapers = { closeDrawerThen(onOpenPapers) },
            )
        },
    ) {
        Scaffold(
            containerColor = c.bg,
            snackbarHost = { SnackbarHost(snackbar) },
            topBar = {
                Column {
                    TopAppBar(
                        title = {
                            if (state.searching) {
                                SearchField(state.search, focusRequester, onChange = vm::setSearch)
                            } else {
                                Text(
                                    buildAnnotatedString {
                                        append(state.selection.title())
                                        state.count?.let { count ->
                                            withStyle(SpanStyle(fontSize = 14.sp, fontWeight = FontWeight.Normal, color = c.text3)) {
                                                append("  $count")
                                            }
                                        }
                                    },
                                    fontSize = 20.sp,
                                    fontWeight = FontWeight.Medium,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        },
                        navigationIcon = {
                            if (state.searching) {
                                IconButton(onClick = vm::closeSearch) {
                                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Cerrar la búsqueda")
                                }
                            } else {
                                IconButton(onClick = { scope.launch { drawerState.open() } }) {
                                    Icon(Icons.Filled.Menu, contentDescription = "Abrir el menú")
                                }
                            }
                        },
                        actions = {
                            if (!state.searching) {
                                IconButton(onClick = vm::openSearch) {
                                    Icon(Icons.Filled.Search, contentDescription = "Buscar")
                                }
                                if (state.selection != ListSelection.Fixed(View.Starred)) {
                                    IconButton(onClick = { vm.markAllRead() }) {
                                        Icon(Icons.Filled.DoneAll, contentDescription = "Marcar todo como leído")
                                    }
                                }
                            }
                        },
                        colors = TopAppBarDefaults.topAppBarColors(containerColor = c.bg),
                    )
                    HorizontalDivider(thickness = 1.dp, color = c.divider)
                }
            },
        ) { padding ->
            Column(Modifier.fillMaxSize().padding(padding)) {
                UpdateBanner(
                    state = update,
                    onUpdate = { startUpdate(context, container.updates) },
                    onClose = container.updates::closeBanner,
                )
                PullToRefreshBox(
                    isRefreshing = state.refreshing,
                    onRefresh = { vm.refresh(pulled = true) },
                    modifier = Modifier.fillMaxWidth().weight(1f),
                ) {
                    when {
                        state.entries.isEmpty() && state.loading -> Centered { CircularProgressIndicator(color = c.text2) }
                        state.entries.isEmpty() && state.error != null -> Centered {
                            Text(state.error.orEmpty(), color = c.text2, textAlign = TextAlign.Center)
                            OutlinedButton(onClick = { vm.refresh() }) { Text("Reintentar") }
                            OutlinedButton(onClick = onOpenSettings) { Text("Ajustes") }
                        }
                        state.entries.isEmpty() && state.sidebar != null -> Centered {
                            Text(emptyText(state.selection, state.search), color = c.text3, textAlign = TextAlign.Center)
                        }
                        else -> LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
                            items(state.entries, key = { it.id }) { entry ->
                                SwipeRow(
                                    entry = entry,
                                    left = settings?.swipeLeft ?: SwipeAction.MarkRead,
                                    right = settings?.swipeRight ?: SwipeAction.Star,
                                    onSwipe = { vm.swipe(entry, it) },
                                ) {
                                    EntryRow(entry, now = now, onClick = { onOpenArticle(entry.id) })
                                }
                            }
                            if (state.loadingMore) {
                                item(key = "more") {
                                    Box(Modifier.fillMaxWidth().padding(16.dp), contentAlignment = Alignment.Center) {
                                        CircularProgressIndicator(color = c.text2, modifier = Modifier.size(24.dp), strokeWidth = 2.dp)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

private fun emptyText(selection: ListSelection, search: String): String = when {
    search.isNotBlank() -> "Ningún artículo coincide con «${search.trim()}»"
    selection == ListSelection.Fixed(View.Unread) -> "No hay nada sin leer"
    selection == ListSelection.Fixed(View.Starred) -> "No hay artículos guardados"
    else -> "No hay artículos"
}

/** Campo de búsqueda que ocupa el lugar del título mientras se busca. */
@Composable
private fun SearchField(text: String, focusRequester: FocusRequester, onChange: (String) -> Unit) {
    val c = Atalaya.colors
    BasicTextField(
        value = text,
        onValueChange = onChange,
        modifier = Modifier.fillMaxWidth().focusRequester(focusRequester),
        singleLine = true,
        textStyle = TextStyle(fontSize = 18.sp, color = c.text),
        cursorBrush = SolidColor(c.accent),
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
        decorationBox = { inner ->
            if (text.isEmpty()) Text("Buscar por título", color = c.text3, fontSize = 18.sp)
            inner()
        },
    )
}

@Composable
private fun Centered(content: @Composable () -> Unit) {
    // PullToRefreshBox necesita algo desplazable debajo para que el gesto funcione.
    LazyColumn(Modifier.fillMaxSize()) {
        item {
            Column(
                Modifier.fillParentMaxSize().padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) { content() }
        }
    }
}
