package dev.rubensalas.atalaya.data.papers

import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.FullPaper
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.Paper
import dev.rubensalas.atalaya.data.api.PaperPage
import dev.rubensalas.atalaya.data.api.PaperQuery
import dev.rubensalas.atalaya.data.api.QueryTranslation
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/** Las rutas de papers del servicio. La implementa [dev.rubensalas.atalaya.data.api.AtalayaClient]. */
interface PapersApi {
    suspend fun translatePaperQuery(q: String): QueryTranslation
    suspend fun searchPapers(query: PaperQuery): PaperPage
    suspend fun paper(arxivId: String): Paper
    suspend fun savePaper(arxivId: String): EntrySummary
    suspend fun fullPaper(arxivId: String): FullPaper
    suspend fun requestFullPaper(arxivId: String): FullPaper
    suspend fun deletePaperTranslation(arxivId: String)
}

/**
 * Búsqueda, lectura y guardado de papers (P1–P5). Siempre contra el servicio:
 * sin red no hay arXiv.
 *
 * Recuerda los papers guardados en esta sesión para que la fila de resultados
 * muestre el marcador al volver del paper, sin repetir la búsqueda.
 */
class PapersRepository(
    private val api: PapersApi,
    /** Se llama tras guardar, para que Guardados lo traiga pronto. */
    private val onSaved: () -> Unit = {},
) {
    private val _saved = MutableStateFlow<Map<String, Id>>(emptyMap())

    /** Identificador de arXiv → artículo de Miniflux, de lo guardado desde que abrió la app. */
    val saved: StateFlow<Map<String, Id>> = _saved.asStateFlow()

    suspend fun translate(q: String): QueryTranslation = api.translatePaperQuery(q)
    suspend fun search(query: PaperQuery): PaperPage = api.searchPapers(query)
    suspend fun paper(arxivId: String): Paper = api.paper(arxivId)
    suspend fun fullPaper(arxivId: String): FullPaper = api.fullPaper(arxivId)
    suspend fun requestFullPaper(arxivId: String): FullPaper = api.requestFullPaper(arxivId)

    /** Borra la traducción del paper para volver a traducirlo (#91). */
    suspend fun deleteTranslation(arxivId: String) = api.deletePaperTranslation(arxivId)

    suspend fun save(arxivId: String): EntrySummary {
        val entry = api.savePaper(arxivId)
        _saved.update { it + (arxivId to entry.id) }
        onSaved()
        return entry
    }
}
