package dev.rubensalas.atalaya.ui.papers

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.Paper
import dev.rubensalas.atalaya.data.api.TranslationState
import dev.rubensalas.atalaya.data.papers.PapersRepository
import dev.rubensalas.atalaya.ui.article.ArticleVersion
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

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
}

/**
 * Un paper abierto desde la búsqueda (pantalla 8, P4). Como un artículo: si el
 * resumen se está traduciendo, lo vuelve a pedir cada [pollInterval] ms.
 */
class PaperViewModel(
    private val arxivId: String,
    private val repo: PapersRepository,
    private val pollInterval: Long = POLL_INTERVAL_MS,
) : ViewModel() {
    private val _state = MutableStateFlow(PaperUiState())
    val state: StateFlow<PaperUiState> = _state.asStateFlow()

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
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.update { it.copy(loading = false, error = errorMessage(e)) }
            }
        }
    }

    /** La pantalla está a la vista. Fuera de ella no se pregunta por la traducción. */
    fun setActive(value: Boolean) {
        active = value
        if (value) startPolling() else pollJob?.cancel()
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

    private fun apply(fresh: Paper) {
        _state.update { s ->
            // Guardado lo manda la app: el servicio puede no saberlo aún si se guardó en esta vuelta.
            val savedId = s.paper?.savedEntryId ?: repo.saved.value[arxivId] ?: fresh.savedEntryId
            s.copy(paper = fresh.copy(savedEntryId = savedId), loading = false, error = null)
        }
        startPolling()
    }

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
