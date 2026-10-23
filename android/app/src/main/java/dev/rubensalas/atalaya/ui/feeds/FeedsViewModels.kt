package dev.rubensalas.atalaya.ui.feeds

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.AtalayaClient
import dev.rubensalas.atalaya.data.api.Category
import dev.rubensalas.atalaya.data.api.DiscoveredFeed
import dev.rubensalas.atalaya.data.api.Feed
import dev.rubensalas.atalaya.data.api.FeedUpdate
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.NewFeed
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

private fun Throwable.userMessage(): String =
    (this as? ApiException)?.message ?: "Algo salió mal"

private fun List<Category>.sortedByTitle() = sortedBy { it.title.lowercase() }

// ---------------------------------------------------------------------------
// Pantalla 9: feeds por carpeta (F5, F6)
// ---------------------------------------------------------------------------

data class FolderGroup(val category: Category, val feeds: List<Feed>)

/** Diálogo de una carpeta abierto con el lápiz: renombrar o borrar (F5). */
data class FolderDialog(val category: Category, val busy: Boolean = false, val error: String? = null)

data class FeedsUiState(
    val loading: Boolean = true,
    val groups: List<FolderGroup> = emptyList(),
    val error: String? = null,
    val folderDialog: FolderDialog? = null,
)

class FeedsViewModel(private val client: AtalayaClient) : ViewModel() {
    private val _state = MutableStateFlow(FeedsUiState())
    val state: StateFlow<FeedsUiState> = _state.asStateFlow()

    /** Se llama cada vez que la pantalla vuelve a verse, para recoger lo que cambió al añadir o editar. */
    fun load() {
        viewModelScope.launch {
            _state.update { it.copy(loading = it.groups.isEmpty(), error = null) }
            try {
                val groups = coroutineScope {
                    val categories = async { client.categories() }
                    val feeds = async { client.feeds() }
                    group(categories.await(), feeds.await())
                }
                _state.update { it.copy(loading = false, groups = groups) }
            } catch (e: ApiException) {
                _state.update { it.copy(loading = false, error = e.userMessage()) }
            }
        }
    }

    fun openFolder(category: Category) = _state.update { it.copy(folderDialog = FolderDialog(category)) }

    fun closeFolder() = _state.update { it.copy(folderDialog = null) }

    fun renameFolder(title: String) = folderAction { client.renameCategory(it.id, title.trim()) }

    /** El servicio responde 409 si la carpeta aún tiene feeds; su mensaje se muestra en el diálogo. */
    fun deleteFolder() = folderAction { client.deleteCategory(it.id) }

    private fun folderAction(action: suspend (Category) -> Unit) {
        val dialog = _state.value.folderDialog ?: return
        _state.update { it.copy(folderDialog = dialog.copy(busy = true, error = null)) }
        viewModelScope.launch {
            try {
                action(dialog.category)
                _state.update { it.copy(folderDialog = null) }
                load()
            } catch (e: ApiException) {
                _state.update { it.copy(folderDialog = dialog.copy(busy = false, error = e.userMessage())) }
            }
        }
    }

    companion object {
        /** Todas las carpetas, también las vacías, para poder borrarlas. */
        fun group(categories: List<Category>, feeds: List<Feed>): List<FolderGroup> {
            val known = categories.associateBy { it.id }
            val all = (categories + feeds.map { it.category }.filter { it.id !in known }.distinctBy { it.id })
            return all.sortedByTitle().map { c ->
                FolderGroup(c, feeds.filter { it.category.id == c.id }.sortedBy { it.title.lowercase() })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pantalla 10: añadir un feed (F1, F2, F3)
// ---------------------------------------------------------------------------

data class AddFeedUiState(
    val url: String = "",
    val searching: Boolean = false,
    /** Null mientras no se ha buscado. */
    val found: List<DiscoveredFeed>? = null,
    val selected: Int = 0,
    val categories: List<Category> = emptyList(),
    val categoryId: Id? = null,
    val crawler: Boolean = false,
    val adding: Boolean = false,
    val error: String? = null,
    val done: Boolean = false,
) {
    val canAdd: Boolean
        get() = !adding && !searching && categoryId != null && !found.isNullOrEmpty()
}

class AddFeedViewModel(private val client: AtalayaClient) : ViewModel() {
    private val _state = MutableStateFlow(AddFeedUiState())
    val state: StateFlow<AddFeedUiState> = _state.asStateFlow()

    init {
        viewModelScope.launch {
            try {
                val categories = client.categories().sortedByTitle()
                _state.update { it.copy(categories = categories, categoryId = it.categoryId ?: categories.firstOrNull()?.id) }
            } catch (e: ApiException) {
                _state.update { it.copy(error = e.userMessage()) }
            }
        }
    }

    /** Cambiar la dirección invalida lo encontrado antes. */
    fun setUrl(url: String) = _state.update { it.copy(url = url, found = null, selected = 0, error = null) }

    fun discover() {
        val url = _state.value.url.trim()
        if (url.isEmpty() || _state.value.searching) return
        _state.update { it.copy(searching = true, found = null, error = null) }
        viewModelScope.launch {
            try {
                val found = client.discoverFeeds(url)
                _state.update {
                    it.copy(
                        searching = false,
                        found = found,
                        selected = 0,
                        error = if (found.isEmpty()) "No se encontró ningún feed en esa dirección" else null,
                    )
                }
            } catch (e: ApiException) {
                _state.update { it.copy(searching = false, error = e.userMessage()) }
            }
        }
    }

    fun select(index: Int) = _state.update { it.copy(selected = index) }

    fun setCategory(id: Id) = _state.update { it.copy(categoryId = id) }

    fun setCrawler(on: Boolean) = _state.update { it.copy(crawler = on) }

    fun createFolder(title: String) {
        viewModelScope.launch {
            try {
                val created = client.addCategory(title.trim())
                _state.update {
                    it.copy(categories = (it.categories + created).sortedByTitle(), categoryId = created.id, error = null)
                }
            } catch (e: ApiException) {
                _state.update { it.copy(error = e.userMessage()) }
            }
        }
    }

    fun add() {
        val s = _state.value
        if (!s.canAdd) return
        val feed = s.found!!.getOrNull(s.selected) ?: return
        _state.update { it.copy(adding = true, error = null) }
        viewModelScope.launch {
            try {
                client.addFeed(NewFeed(feedUrl = feed.feedUrl, categoryId = s.categoryId!!, crawler = s.crawler))
                _state.update { it.copy(adding = false, done = true) }
            } catch (e: ApiException) {
                _state.update { it.copy(adding = false, error = e.userMessage()) }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pantalla 11: editar un feed (F4, F6)
// ---------------------------------------------------------------------------

data class EditFeedUiState(
    val loading: Boolean = true,
    val feed: Feed? = null,
    val categories: List<Category> = emptyList(),
    val title: String = "",
    val categoryId: Id? = null,
    val crawler: Boolean = false,
    val saving: Boolean = false,
    val confirmDelete: Boolean = false,
    val error: String? = null,
    val done: Boolean = false,
) {
    /** Solo lo que cambió respecto al feed cargado. Null si no cambió nada. */
    val update: FeedUpdate?
        get() {
            val f = feed ?: return null
            val u = FeedUpdate(
                title = title.trim().takeIf { it != f.title },
                categoryId = categoryId.takeIf { it != f.category.id },
                crawler = crawler.takeIf { it != f.crawler },
            )
            return u.takeIf { it != FeedUpdate() }
        }

    val canSave: Boolean
        get() = feed != null && !saving && title.isNotBlank()
}

class EditFeedViewModel(private val client: AtalayaClient, private val id: Id) : ViewModel() {
    private val _state = MutableStateFlow(EditFeedUiState())
    val state: StateFlow<EditFeedUiState> = _state.asStateFlow()

    init {
        load()
    }

    fun load() {
        _state.update { it.copy(loading = true, error = null) }
        viewModelScope.launch {
            try {
                // El servicio no tiene GET /api/feeds/{id}: se busca en la lista.
                val (feeds, categories) = coroutineScope {
                    val feeds = async { client.feeds() }
                    val categories = async { client.categories() }
                    feeds.await() to categories.await()
                }
                val feed = feeds.firstOrNull { it.id == id }
                _state.update {
                    if (feed == null) {
                        it.copy(loading = false, error = "Este feed ya no existe")
                    } else {
                        it.copy(
                            loading = false,
                            feed = feed,
                            categories = categories.sortedByTitle(),
                            title = feed.title,
                            categoryId = feed.category.id,
                            crawler = feed.crawler,
                        )
                    }
                }
            } catch (e: ApiException) {
                _state.update { it.copy(loading = false, error = e.userMessage()) }
            }
        }
    }

    fun setTitle(title: String) = _state.update { it.copy(title = title) }

    fun setCategory(id: Id) = _state.update { it.copy(categoryId = id) }

    fun setCrawler(on: Boolean) = _state.update { it.copy(crawler = on) }

    fun save() {
        val s = _state.value
        if (!s.canSave) return
        val update = s.update ?: run {
            _state.update { it.copy(done = true) }
            return
        }
        _state.update { it.copy(saving = true, error = null) }
        viewModelScope.launch {
            try {
                client.updateFeed(id, update)
                _state.update { it.copy(saving = false, done = true) }
            } catch (e: ApiException) {
                _state.update { it.copy(saving = false, error = e.userMessage()) }
            }
        }
    }

    fun askDelete(ask: Boolean) = _state.update { it.copy(confirmDelete = ask) }

    fun delete() {
        _state.update { it.copy(confirmDelete = false, saving = true, error = null) }
        viewModelScope.launch {
            try {
                client.deleteFeed(id)
                _state.update { it.copy(saving = false, done = true) }
            } catch (e: ApiException) {
                _state.update { it.copy(saving = false, error = e.userMessage()) }
            }
        }
    }
}
