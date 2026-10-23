package dev.rubensalas.atalaya.ui.list

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.api.View
import dev.rubensalas.atalaya.data.list.ListRepository
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
     * para no perder lo cargado al volver de un artículo.
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
                    )
                }
                repo.markSynced()
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.update { it.copy(loading = false, refreshing = false, error = errorMessage(e)) }
            }
        }
    }

    /** Pide la página siguiente cuando la lista llega al final. */
    fun loadMore() {
        val current = _state.value
        if (current.loading || current.loadingMore || !current.canLoadMore) return
        _state.update { it.copy(loadingMore = true) }
        moreJob = viewModelScope.launch {
            try {
                val page = repo.entries(current.selection.query(PAGE_SIZE, current.entries.size))
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

    private fun errorMessage(e: ApiException): String =
        if (e.status == 401) "La clave no es correcta. Revísala en Ajustes." else e.message.orEmpty()

    companion object {
        const val PAGE_SIZE = 50
        const val MAX_PAGE_SIZE = 200
    }
}
