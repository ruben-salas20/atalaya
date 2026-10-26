package dev.rubensalas.atalaya.ui.article

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.EntryUpdate
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.Progress
import dev.rubensalas.atalaya.data.api.TranslationState
import dev.rubensalas.atalaya.data.list.ListRepository
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

/** Qué versión del texto se ve: el selector de arriba (L5). */
enum class ArticleVersion { Spanish, Original }

data class ArticleUiState(
    val entry: Entry? = null,
    val loading: Boolean = true,
    /** Fallo sin artículo que enseñar. */
    val error: String? = null,
    /** Aviso de abajo que se va solo. */
    val message: String? = null,
    val original: List<Block> = emptyList(),
    /** El español: completo, o lo traducido hasta ahora mientras se traduce (ADR 0015). */
    val translated: List<Block>? = null,
    /** Lo que eligió Rubén en el selector. Sin elegir, se ve el español en cuanto existe. */
    val chosen: ArticleVersion? = null,
) {
    val translating: Boolean get() = entry?.translation == TranslationState.Pending

    /** Avance de la traducción, solo mientras se traduce y el servicio lo dice. */
    val progress: Progress? get() = entry?.progress?.takeIf { translating }

    /** El selector se ve si el artículo no estaba ya en español, y se puede tocar si hay traducción. */
    val showSelector: Boolean get() = entry != null && entry.translation != TranslationState.NotNeeded
    val canSwitch: Boolean get() = translated != null

    val showing: ArticleVersion
        get() = if (translated == null) ArticleVersion.Original else chosen ?: ArticleVersion.Spanish

    val blocks: List<Block>
        get() = if (showing == ArticleVersion.Spanish) translated.orEmpty() else original

    val title: String
        get() {
            val e = entry ?: return ""
            return when {
                showing == ArticleVersion.Spanish -> e.title
                e.translation == TranslationState.NotNeeded -> e.title
                else -> e.originalTitle
            }
        }
}

/**
 * Carga un artículo, lo marca como leído y, mientras se traduce, lo vuelve a
 * pedir cada [pollInterval] ms hasta que llega el español (L5, L6).
 */
class ArticleViewModel(
    private val id: Id,
    private val repo: ListRepository,
    private val parseDispatcher: CoroutineDispatcher = Dispatchers.Default,
    private val pollInterval: Long = POLL_INTERVAL_MS,
) : ViewModel() {
    private val _state = MutableStateFlow(ArticleUiState())
    val state: StateFlow<ArticleUiState> = _state.asStateFlow()

    private var active = false
    private var pollJob: Job? = null

    /** Si Rubén lo marca como no leído, abrirlo no vuelve a marcarlo. */
    private var readHandled = false

    init {
        load()
    }

    fun load() {
        _state.update { it.copy(loading = true, error = null) }
        viewModelScope.launch {
            try {
                apply(repo.entry(id))
                markReadOnOpen()
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

    fun toggleStarred() {
        val entry = _state.value.entry ?: return
        change(EntryUpdate(starred = !entry.starred), if (entry.starred) "No se pudo quitar de guardados" else "No se pudo guardar")
    }

    fun toggleRead() {
        val entry = _state.value.entry ?: return
        readHandled = true
        change(EntryUpdate(read = !entry.read), "No se pudo cambiar el estado de lectura")
    }

    fun dismissMessage() = _state.update { it.copy(message = null) }

    private fun markReadOnOpen() {
        val entry = _state.value.entry ?: return
        if (readHandled) return
        readHandled = true
        // Si falla, el artículo se queda sin leer en Miniflux; no merece un aviso.
        if (!entry.read) change(EntryUpdate(read = true), failure = null)
    }

    /** Cambia leído o guardado al momento y lo deshace si el servicio falla. */
    private fun change(update: EntryUpdate, failure: String?) {
        val before = _state.value.entry ?: return
        setFlags(update.read ?: before.read, update.starred ?: before.starred)
        viewModelScope.launch {
            try {
                val after = repo.updateEntry(id, update)
                setFlags(after.read, after.starred)
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                setFlags(before.read, before.starred)
                if (failure != null) _state.update { it.copy(message = failure) }
            }
        }
    }

    private fun setFlags(read: Boolean, starred: Boolean) =
        _state.update { s -> s.copy(entry = s.entry?.copy(read = read, starred = starred)) }

    private suspend fun apply(fresh: Entry) {
        val current = _state.value
        val original = if (current.entry?.content == fresh.content) current.original
        else withContext(parseDispatcher) { parseArticleHtml(fresh.content, fresh.url) }
        // Con Pending llega lo traducido hasta ahora (ADR 0015); cada vuelta puede traer más, así que se vuelve a leer si cambió.
        val translated = fresh.translatedContent
            ?.takeIf { fresh.translation == TranslationState.Done || fresh.translation == TranslationState.Pending }
            ?.let { html ->
                if (current.entry?.translatedContent == html && current.translated != null) current.translated
                else withContext(parseDispatcher) { parseArticleHtml(html, fresh.url) }
            }
        _state.update { s ->
            // Leído y guardado los manda la app: puede haber un cambio en camino.
            val entry = s.entry?.let { fresh.copy(read = it.read, starred = it.starred) } ?: fresh
            s.copy(entry = entry, loading = false, error = null, original = original, translated = translated)
        }
        startPolling()
    }

    private fun startPolling() {
        if (!active || !_state.value.translating || pollJob?.isActive == true) return
        pollJob = viewModelScope.launch {
            while (_state.value.translating) {
                delay(pollInterval)
                try {
                    apply(repo.entry(id))
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
        404 -> "Este artículo ya no existe en Miniflux."
        else -> e.message.orEmpty()
    }

    companion object {
        const val POLL_INTERVAL_MS = 3_000L
    }
}

/** «Traduciendo… 12 de 52». */
fun translationProgressText(progress: Progress): String = "Traduciendo… ${progress.done} de ${progress.total}"

/** La fracción de la barra, entre 0 y 1. Sin párrafos, 0. */
fun translationFraction(progress: Progress): Float =
    if (progress.total <= 0) 0f else (progress.done.toFloat() / progress.total).coerceIn(0f, 1f)
