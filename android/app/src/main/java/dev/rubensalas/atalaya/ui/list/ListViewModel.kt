package dev.rubensalas.atalaya.ui.list

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.EntryUpdate
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.MarkAllReadQuery
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.api.View
import dev.rubensalas.atalaya.data.list.ListRepository
import dev.rubensalas.atalaya.data.list.adjusted
import dev.rubensalas.atalaya.data.settings.SwipeAction
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Qué lista se ve (L1): una de las tres fijas, una carpeta o un feed. */
sealed interface ListSelection {
    data class Fixed(val view: View) : ListSelection
    data class Category(val id: Id, val title: String) : ListSelection
    data class Feed(val id: Id, val title: String) : ListSelection

    // En una carpeta o un feed se ven todos sus artículos; los leídos, en gris (L3).
    fun query(limit: Int, offset: Int): EntryQuery = when (this) {
        is Fixed -> EntryQuery(view = view, limit = limit, offset = offset)
        is Category -> EntryQuery(view = View.All, categoryId = id, limit = limit, offset = offset)
        is Feed -> EntryQuery(view = View.All, feedId = id, limit = limit, offset = offset)
    }
}

data class ListUiState(
    val selection: ListSelection = ListSelection.Fixed(View.Unread),
    val sidebar: Sidebar? = null,
    val entries: List<EntrySummary> = emptyList(),
    val total: Long = 0,
    /** Carga sin nada que enseñar todavía. */
    val loading: Boolean = false,
    /** El usuario tiró de la lista hacia abajo. */
    val refreshing: Boolean = false,
    val loadingMore: Boolean = false,
    val error: String? = null,
    /** Sin conexión: se ve lo guardado en el móvil (L8). */
    val offline: Boolean = false,
) {
    val canLoadMore: Boolean get() = entries.size < total

    /** El número junto al título: el mismo que muestra el menú lateral. */
    val count: Long?
        get() {
            val s = sidebar ?: return null
            return when (val sel = selection) {
                is ListSelection.Fixed -> when (sel.view) {
                    View.All -> s.total
                    View.Unread -> s.unread
                    View.Starred -> s.starred
                }
                is ListSelection.Category -> s.categories.firstOrNull { it.id == sel.id }?.unread
                is ListSelection.Feed -> s.categories.flatMap { it.feeds }.firstOrNull { it.id == sel.id }?.unread
            }
        }
}

/**
 * Cambia leído y guardado de un artículo de la lista y ajusta los números del
 * menú lateral, para que todo cuadre sin volver a pedirlo.
 */
fun ListUiState.withFlags(id: Id, read: Boolean, starred: Boolean): ListUiState {
    val old = entries.firstOrNull { it.id == id } ?: return this
    if (old.read == read && old.starred == starred) return this
    val unreadDelta = (if (read) 0 else 1) - (if (old.read) 0 else 1)
    val starredDelta = (if (starred) 1 else 0) - (if (old.starred) 1 else 0)
    return copy(
        entries = entries.map { if (it.id == id) it.copy(read = read, starred = starred) else it },
        sidebar = sidebar?.adjusted(old.feedId, unreadDelta, starredDelta),
    )
}

/** Carga la lista y el menú lateral (pantallas 1 y 2). */
class ListViewModel(private val repo: ListRepository) : ViewModel() {
    private val _state = MutableStateFlow(ListUiState())
    val state: StateFlow<ListUiState> = _state.asStateFlow()

    private var loadJob: Job? = null
    private var moreJob: Job? = null

    fun select(selection: ListSelection) {
        if (selection == _state.value.selection) return
        _state.update { it.copy(selection = selection, entries = emptyList(), total = 0, error = null) }
        refresh()
    }

    /**
     * Vuelve a pedir el menú y la lista. Pide tantos artículos como ya se ven,
     * para no perder lo cargado al volver de un artículo. Si la lista está
     * vacía, enseña antes lo guardado en el móvil. Sin conexión, se queda con
     * lo guardado y lo avisa una vez (L8).
     */
    fun refresh(pulled: Boolean = false) {
        loadJob?.cancel()
        moreJob?.cancel()
        val selection = _state.value.selection
        val limit = _state.value.entries.size.coerceIn(PAGE_SIZE, MAX_PAGE_SIZE)
        _state.update {
            it.copy(loading = it.entries.isEmpty(), refreshing = pulled, loadingMore = false, error = null)
        }
        loadJob = viewModelScope.launch {
            if (_state.value.entries.isEmpty()) showCached(selection, limit, offline = false)
            try {
                val (sidebar, page) = coroutineScope {
                    val sidebar = async { repo.sidebar() }
                    val page = async { repo.entries(selection.query(limit, 0)) }
                    sidebar.await() to page.await()
                }
                _state.update {
                    it.copy(
                        sidebar = sidebar,
                        entries = page.entries,
                        total = page.total,
                        loading = false,
                        refreshing = false,
                        offline = false,
                    )
                }
                repo.markSynced()
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                if (e.status == null && showCached(selection, limit, offline = true)) return@launch
                _state.update { it.copy(loading = false, refreshing = false, error = errorMessage(e)) }
            }
        }
    }

    /** Enseña lo guardado en el móvil. Devuelve false si no hay nada guardado. */
    private suspend fun showCached(selection: ListSelection, limit: Int, offline: Boolean): Boolean {
        val page = repo.cachedEntries(selection.query(limit, 0)) ?: return false
        // Antes de la red, una lista vacía solo haría parpadear el «no hay artículos».
        if (!offline && page.entries.isEmpty()) return false
        val sidebar = repo.cachedSidebar()
        _state.update {
            it.copy(
                sidebar = sidebar ?: it.sidebar,
                entries = page.entries,
                total = page.total,
                loading = false,
                refreshing = if (offline) false else it.refreshing,
                offline = offline,
                // El aviso va abajo y solo con artículos en pantalla; si no, taparía la lista vacía.
                error = if (offline && !it.offline && page.entries.isNotEmpty()) OFFLINE_NOTICE else it.error,
            )
        }
        return true
    }

    /** Pide la página siguiente cuando la lista llega al final. Sin conexión, la saca del móvil. */
    fun loadMore() {
        val current = _state.value
        if (current.loading || current.loadingMore || !current.canLoadMore) return
        _state.update { it.copy(loadingMore = true) }
        moreJob = viewModelScope.launch {
            val query = current.selection.query(PAGE_SIZE, current.entries.size)
            try {
                val page = try {
                    repo.entries(query)
                } catch (e: ApiException) {
                    if (e.status != null) throw e
                    repo.cachedEntries(query) ?: throw e
                }
                _state.update { s ->
                    val known = s.entries.mapTo(HashSet()) { it.id }
                    s.copy(
                        entries = s.entries + page.entries.filter { it.id !in known },
                        total = page.total,
                        loadingMore = false,
                    )
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.update { it.copy(loadingMore = false, error = errorMessage(e)) }
            }
        }
    }

    fun dismissError() = _state.update { it.copy(error = null) }

    /**
     * Deslizar una fila (L4). Cada gesto alterna: marca como leído o como no
     * leído, guarda o quita de guardados. La fila cambia al momento y vuelve
     * atrás si el servicio falla; la lista no se recarga.
     */
    fun swipe(entry: EntrySummary, action: SwipeAction) {
        val update = when (action) {
            SwipeAction.MarkRead -> EntryUpdate(read = !entry.read)
            SwipeAction.Star -> EntryUpdate(starred = !entry.starred)
            SwipeAction.None -> return
        }
        val before = _state.value.entries.firstOrNull { it.id == entry.id } ?: return
        _state.update { it.withFlags(entry.id, update.read ?: before.read, update.starred ?: before.starred) }
        viewModelScope.launch {
            try {
                val after = repo.updateEntry(entry.id, update)
                _state.update { it.withFlags(entry.id, after.read, after.starred) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.update { it.withFlags(entry.id, before.read, before.starred).copy(error = "No se pudo cambiar: ${errorMessage(e)}") }
            }
        }
    }

    /** Marca como leído lo de la lista abierta. «Destacados» no tiene este botón. */
    fun markAllRead() {
        val scope = when (val selection = _state.value.selection) {
            is ListSelection.Fixed -> if (selection.view == View.Starred) return else MarkAllReadQuery()
            is ListSelection.Category -> MarkAllReadQuery(categoryId = selection.id)
            is ListSelection.Feed -> MarkAllReadQuery(feedId = selection.id)
        }
        viewModelScope.launch {
            try {
                repo.markAllRead(scope)
                refresh()
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.update { it.copy(error = "No se pudo marcar: ${errorMessage(e)}") }
            }
        }
    }

    private fun errorMessage(e: ApiException): String =
        if (e.status == 401) "La clave no es correcta. Revísala en Ajustes." else e.message.orEmpty()

    companion object {
        const val PAGE_SIZE = 50
        const val MAX_PAGE_SIZE = 200
        const val OFFLINE_NOTICE = "Sin conexión. Ves lo guardado en el móvil."
    }
}
