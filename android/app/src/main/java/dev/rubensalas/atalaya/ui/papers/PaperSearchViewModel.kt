package dev.rubensalas.atalaya.ui.papers

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.PaperField
import dev.rubensalas.atalaya.data.api.PaperQuery
import dev.rubensalas.atalaya.data.api.PaperSort
import dev.rubensalas.atalaya.data.api.PaperSummary
import dev.rubensalas.atalaya.data.papers.PapersRepository
import dev.rubensalas.atalaya.data.papers.RecentSearch
import dev.rubensalas.atalaya.data.papers.RecentSearches
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class PaperSearchUiState(
    val text: String = "",
    val field: PaperField = PaperField.All,
    val sort: PaperSort = PaperSort.Recent,
    /** «Se buscará en inglés: …» (P1). Null si no hay texto o se busca por autor. */
    val english: String? = null,
    val recents: List<RecentSearch> = emptyList(),
    /** La búsqueda que dio [papers]. Null si aún no se buscó. */
    val submitted: PaperQuery? = null,
    val englishQuery: String = "",
    val total: Long = 0,
    val papers: List<PaperSummary> = emptyList(),
    val searching: Boolean = false,
    val loadingMore: Boolean = false,
    val error: String? = null,
    /** Rubén está escribiendo o eligiendo filtros: se ve la pantalla 6, no los resultados. */
    val editing: Boolean = true,
) {
    val showResults: Boolean get() = !editing && submitted != null
    val canLoadMore: Boolean get() = submitted != null && !searching && !loadingMore && error == null && papers.size < total
}

/**
 * Buscar papers (pantallas 6 y 7). Mientras se escribe, pide la búsqueda en
 * inglés tras [debounceMs] sin teclear (P1).
 */
class PaperSearchViewModel(
    private val repo: PapersRepository,
    private val recents: RecentSearches,
    private val debounceMs: Long = DEBOUNCE_MS,
) : ViewModel() {
    private val _state = MutableStateFlow(PaperSearchUiState())
    val state: StateFlow<PaperSearchUiState> = _state.asStateFlow()

    private var previewJob: Job? = null
    private var searchJob: Job? = null

    init {
        viewModelScope.launch { recents.searches.collect { list -> _state.update { it.copy(recents = list) } } }
        viewModelScope.launch {
            repo.saved.collect { saved ->
                if (saved.isEmpty()) return@collect
                _state.update { s ->
                    s.copy(papers = s.papers.map { p -> saved[p.arxivId]?.let { p.copy(savedEntryId = it) } ?: p })
                }
            }
        }
    }

    fun setText(text: String) {
        if (text == _state.value.text) return
        _state.update { it.copy(text = text, editing = true) }
        schedulePreview()
    }

    /** El campo de búsqueda tomó el foco: vuelve la pantalla 6. */
    fun startEditing() = _state.update { it.copy(editing = true) }

    fun setField(field: PaperField) {
        _state.update { it.copy(field = field, editing = true) }
        schedulePreview()
    }

    fun setSort(sort: PaperSort) = _state.update { it.copy(sort = sort, editing = true) }

    fun search() {
        val s = _state.value
        val q = s.text.trim()
        if (q.isEmpty()) return
        val query = PaperQuery(q = q, field = s.field, sort = s.sort, limit = PAGE_SIZE)
        viewModelScope.launch { recents.add(RecentSearch(q, s.field)) }
        searchJob?.cancel()
        _state.update {
            it.copy(submitted = query, editing = false, searching = true, loadingMore = false, error = null, papers = emptyList(), total = 0)
        }
        searchJob = viewModelScope.launch {
            try {
                val page = repo.search(query)
                _state.update {
                    it.copy(searching = false, englishQuery = page.englishQuery, total = page.total, papers = withSaved(page.papers))
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.update { it.copy(searching = false, error = errorMessage(e)) }
            }
        }
    }

    fun searchRecent(recent: RecentSearch) {
        _state.update { it.copy(text = recent.q, field = recent.field) }
        previewJob?.cancel()
        search()
        schedulePreview()
    }

    fun removeRecent(recent: RecentSearch) {
        viewModelScope.launch { recents.remove(recent) }
    }

    /** La siguiente página, al llegar al final de la lista. */
    fun loadMore() {
        val s = _state.value
        val query = s.submitted ?: return
        if (!s.canLoadMore) return
        _state.update { it.copy(loadingMore = true) }
        searchJob = viewModelScope.launch {
            try {
                val page = repo.search(query.copy(offset = s.papers.size))
                _state.update { cur ->
                    val known = cur.papers.mapTo(HashSet()) { it.arxivId }
                    val more = withSaved(page.papers).filterNot { it.arxivId in known }
                    // Si arXiv no da nada nuevo, se da por acabada la lista.
                    val total = if (more.isEmpty()) cur.papers.size.toLong() else page.total
                    cur.copy(loadingMore = false, total = total, papers = cur.papers + more)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.update { it.copy(loadingMore = false, error = errorMessage(e)) }
            }
        }
    }

    private fun withSaved(papers: List<PaperSummary>): List<PaperSummary> {
        val saved = repo.saved.value
        return papers.map { p -> saved[p.arxivId]?.let { p.copy(savedEntryId = it) } ?: p }
    }

    private fun schedulePreview() {
        previewJob?.cancel()
        val s = _state.value
        val q = s.text.trim()
        if (q.isEmpty() || s.field == PaperField.Author) {
            _state.update { it.copy(english = null) }
            return
        }
        previewJob = viewModelScope.launch {
            delay(debounceMs)
            try {
                val t = repo.translate(q)
                _state.update { cur ->
                    if (cur.text.trim() == q && cur.field != PaperField.Author) cur.copy(english = t.english) else cur
                }
            } catch (e: CancellationException) {
                throw e
            } catch (_: ApiException) {
                // Sin la vista previa se puede buscar igual.
            }
        }
    }

    private fun errorMessage(e: ApiException): String = when (e.status) {
        401 -> "La clave no es correcta. Revísala en Ajustes."
        else -> e.message.orEmpty()
    }

    companion object {
        const val DEBOUNCE_MS = 600L
        const val PAGE_SIZE = 20
    }
}
