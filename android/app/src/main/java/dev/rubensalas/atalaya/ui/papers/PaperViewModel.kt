package dev.rubensalas.atalaya.ui.papers

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.Paper
import dev.rubensalas.atalaya.data.api.TranslationState
import dev.rubensalas.atalaya.data.list.ListRepository
import dev.rubensalas.atalaya.data.papers.PapersRepository
import dev.rubensalas.atalaya.ui.article.ArticleVersion
import dev.rubensalas.atalaya.ui.article.Block
import dev.rubensalas.atalaya.ui.article.FullTextController
import dev.rubensalas.atalaya.ui.article.FullTextState
import dev.rubensalas.atalaya.ui.article.parseArticleHtml
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

data class PaperUiState(
    val paper: Paper? = null,
    val loading: Boolean = true,
    /** Fallo sin paper que enseñar. */
    val error: String? = null,
    /** Aviso de abajo que se va solo. */
    val message: String? = null,
    val saving: Boolean = false,
    /** Lo que eligió Rubén en el selector. Sin elegir, se ve el español en cuanto existe. */
    val chosen: ArticleVersion? = null,
    /** El resumen original en bloques, si el servicio mandó HTML con fórmulas (#80). */
    val originalAbstractBlocks: List<Block> = emptyList(),
    /** Lo mismo para la traducción. */
    val translatedAbstractBlocks: List<Block> = emptyList(),
) {
    val translating: Boolean get() = paper?.translation == TranslationState.Pending

    val showSelector: Boolean get() = paper != null && paper.translation != TranslationState.NotNeeded
    private val translated: String? get() = paper?.translatedAbstract?.takeIf { paper.translation == TranslationState.Done }
    val canSwitch: Boolean get() = translated != null

    val showing: ArticleVersion
        get() = if (translated == null) ArticleVersion.Original else chosen ?: ArticleVersion.Spanish

    val saved: Boolean get() = paper?.savedEntryId != null

    val title: String
        get() {
            val p = paper ?: return ""
            return when {
                showing == ArticleVersion.Spanish -> p.title
                p.translation == TranslationState.NotNeeded -> p.title
                else -> p.originalTitle
            }
        }

    val paragraphs: List<String>
        get() = abstractParagraphs((if (showing == ArticleVersion.Spanish) translated else paper?.abstractText).orEmpty())

    /**
     * Los bloques del resumen que se ven, con sus fórmulas. Null si el servicio
     * no mandó HTML para esa versión: entonces se pinta [paragraphs], como antes.
     */
    val showingAbstractBlocks: List<Block>?
        get() = (if (showing == ArticleVersion.Spanish) translatedAbstractBlocks else originalAbstractBlocks).takeIf { it.isNotEmpty() }
}

/**
 * Un paper abierto desde la búsqueda (pantalla 8, P4). Como un artículo: si el
 * resumen se está traduciendo, lo vuelve a pedir cada [pollInterval] ms.
 */
class PaperViewModel(
    private val arxivId: String,
    private val repo: PapersRepository,
    /** Para la copia del artículo guardado, si el paper está en Guardados (#91). */
    private val list: ListRepository,
    /** Dirección del servicio. Las fórmulas del paper completo son rutas relativas a ella (ADR 0010). */
    private val serviceUrl: suspend () -> String = { "" },
    private val parseDispatcher: CoroutineDispatcher = Dispatchers.Default,
    private val pollInterval: Long = POLL_INTERVAL_MS,
) : ViewModel() {
    private val _state = MutableStateFlow(PaperUiState())
    val state: StateFlow<PaperUiState> = _state.asStateFlow()

    /** El paper completo (P7): lo comparte con el artículo de un paper, ver [FullTextController]. */
    private val fullText = FullTextController(viewModelScope, repo, serviceUrl, parseDispatcher, pollInterval)
    val full: StateFlow<FullTextState> = fullText.state

    private var active = false
    private var pollJob: Job? = null

    init {
        load()
    }

    fun load() {
        _state.update { it.copy(loading = true, error = null) }
        viewModelScope.launch {
            try {
                apply(repo.paper(arxivId))
                fullText.open(arxivId)
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.update { it.copy(loading = false, error = errorMessage(e)) }
            }
        }
    }

    /** «Leer completo» (P7): pide el paper entero. También sirve para reintentar tras un fallo. */
    fun readFull() = fullText.readFull(onFailed = { _state.update { it.copy(message = "No se pudo pedir el paper completo") } })

    /** La pantalla está a la vista. Fuera de ella no se pregunta por la traducción ni por el paper completo. */
    fun setActive(value: Boolean) {
        active = value
        fullText.setActive(value)
        if (value) startPolling() else pollJob?.cancel()
    }

    /**
     * «Volver a traducir» (#91): borra la traducción del paper, texto completo incluido,
     * y la del artículo guardado si lo hay. Con 409 no borra nada y lo dice.
     */
    fun retranslate() {
        val paper = _state.value.paper ?: return
        viewModelScope.launch {
            var forgotten = false
            try {
                repo.deleteTranslation(arxivId)
                forgotten = true
                paper.savedEntryId?.let { list.deleteTranslation(it) }
                load()
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                if (forgotten) load()
                _state.update { it.copy(message = e.message.orEmpty()) }
            }
        }
    }

    fun choose(version: ArticleVersion) {
        if (_state.value.canSwitch) _state.update { it.copy(chosen = version) }
    }

    /** Lo lleva a Guardados (P5). Si ya estaba, no hace nada. */
    fun save() {
        val s = _state.value
        if (s.paper == null || s.saved || s.saving) return
        _state.update { it.copy(saving = true) }
        viewModelScope.launch {
            try {
                val entry = repo.save(arxivId)
                _state.update { cur ->
                    cur.copy(saving = false, paper = cur.paper?.copy(savedEntryId = entry.id))
                }
            } catch (e: CancellationException) {
                throw e
            } catch (_: ApiException) {
                _state.update { it.copy(saving = false, message = "No se pudo guardar") }
            }
        }
    }

    fun dismissMessage() = _state.update { it.copy(message = null) }

    private suspend fun apply(fresh: Paper) {
        val before = _state.value
        val base = serviceUrl()
        // Como el paper completo: el HTML del resumen se vuelve a leer solo si cambió (ADR 0015).
        val original = if (fresh.abstractHtml == before.paper?.abstractHtml) before.originalAbstractBlocks
        else parseAbstract(fresh.abstractHtml, base)
        val translated = if (fresh.translatedAbstractHtml == before.paper?.translatedAbstractHtml) before.translatedAbstractBlocks
        else parseAbstract(fresh.translatedAbstractHtml, base)
        _state.update { s ->
            // Guardado lo manda la app: el servicio puede no saberlo aún si se guardó en esta vuelta.
            val savedId = s.paper?.savedEntryId ?: repo.saved.value[arxivId] ?: fresh.savedEntryId
            s.copy(
                paper = fresh.copy(savedEntryId = savedId),
                loading = false,
                error = null,
                originalAbstractBlocks = original,
                translatedAbstractBlocks = translated,
            )
        }
        startPolling()
    }

    /** Los bloques de un resumen en HTML, con las fórmulas resueltas contra el servicio. Sin HTML, ninguno. */
    private suspend fun parseAbstract(html: String?, base: String): List<Block> =
        if (html.isNullOrBlank()) emptyList() else withContext(parseDispatcher) { parseArticleHtml(html, base) }

    private fun startPolling() {
        if (!active || !_state.value.translating || pollJob?.isActive == true) return
        pollJob = viewModelScope.launch {
            while (_state.value.translating) {
                delay(pollInterval)
                try {
                    apply(repo.paper(arxivId))
                } catch (e: CancellationException) {
                    throw e
                } catch (_: ApiException) {
                    // Sin red un momento: se vuelve a probar en la siguiente vuelta.
                }
            }
        }
    }

    private fun errorMessage(e: ApiException): String = when (e.status) {
        401 -> "La clave no es correcta. Revísala en Ajustes."
        404 -> "Este paper no existe en arXiv."
        else -> e.message.orEmpty()
    }

    companion object {
        const val POLL_INTERVAL_MS = 3_000L
    }
}
