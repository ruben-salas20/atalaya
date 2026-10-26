package dev.rubensalas.atalaya.ui.article

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.FullPaper
import dev.rubensalas.atalaya.data.api.FullPaperState
import dev.rubensalas.atalaya.data.api.Progress
import dev.rubensalas.atalaya.data.papers.PapersRepository
import dev.rubensalas.atalaya.ui.theme.Atalaya
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Lo que muestra «Texto completo» de un paper (P7, ADR 0010). Lo usan el paper y el artículo de un paper. */
data class FullTextState(
    val full: FullPaper? = null,
    /** El texto completo en bloques. Se vuelve a leer solo si cambia el HTML. */
    val blocks: List<Block> = emptyList(),
    /** Se acaba de pedir el paper completo y el servicio aún no contestó. */
    val requesting: Boolean = false,
) {
    val state: FullPaperState get() = full?.state ?: FullPaperState.NotRequested

    /** Avance, solo mientras se traduce y el servicio lo dice. */
    val progress: Progress? get() = full?.progress?.takeIf { state == FullPaperState.Pending }

    /** Hay que consultar cada pocos segundos mientras se traduce. */
    val polling: Boolean get() = state == FullPaperState.Pending
}

/**
 * El paper completo de un arXiv id. Lo lee del servicio al abrirlo, lo pide con «Leer
 * completo» y, mientras se traduce, lo vuelve a pedir cada [pollInterval] ms. Solo
 * consulta mientras [setActive] está en true: fuera de la pantalla, para.
 */
class FullTextController(
    private val scope: CoroutineScope,
    private val papers: PapersRepository,
    /** Dirección del servicio. Las fórmulas del paper completo son rutas relativas a ella (ADR 0010). */
    private val serviceUrl: suspend () -> String,
    private val parseDispatcher: CoroutineDispatcher,
    private val pollInterval: Long,
) {
    private val _state = MutableStateFlow(FullTextState())
    val state: StateFlow<FullTextState> = _state.asStateFlow()

    private var arxivId: String? = null
    private var active = false
    private var pollJob: Job? = null

    /** Abre el paper: lee su estado del servicio. Sin llamarlo, nunca se pregunta por el texto completo. */
    fun open(arxivId: String) {
        this.arxivId = arxivId
        scope.launch { fetch() }
    }

    /** «Leer completo»: pide el paper entero. También sirve para reintentar tras un fallo. */
    fun readFull(onFailed: () -> Unit) {
        val id = arxivId ?: return
        val s = _state.value
        val askable = s.state == FullPaperState.NotRequested || s.state == FullPaperState.Failed
        if (!askable || s.requesting) return
        _state.update { it.copy(requesting = true) }
        scope.launch {
            try {
                apply(papers.requestFullPaper(id))
            } catch (e: CancellationException) {
                throw e
            } catch (_: ApiException) {
                _state.update { it.copy(requesting = false) }
                onFailed()
            }
        }
    }

    /** La pantalla está a la vista. Fuera de ella no se consulta. */
    fun setActive(value: Boolean) {
        active = value
        if (value) startPolling() else pollJob?.cancel()
    }

    /** Sin red o sin respuesta, se vuelve a probar en la siguiente vuelta. */
    private suspend fun fetch() {
        val id = arxivId ?: return
        try {
            apply(papers.fullPaper(id))
        } catch (e: CancellationException) {
            throw e
        } catch (_: ApiException) {
            // Se vuelve a probar en la siguiente vuelta.
        }
    }

    /** Mientras el servicio traduce, el HTML llega a trozos: se vuelve a leer solo si cambió (ADR 0015). */
    private suspend fun apply(fresh: FullPaper) {
        val current = _state.value
        val base = serviceUrl()
        val blocks = if (fresh.html == current.full?.html) current.blocks
        else withContext(parseDispatcher) { parseArticleHtml(fresh.html.orEmpty(), base) }
        _state.update { it.copy(full = fresh, blocks = blocks, requesting = false) }
        startPolling()
    }

    private fun startPolling() {
        if (!active || !_state.value.polling || pollJob?.isActive == true) return
        pollJob = scope.launch {
            while (_state.value.polling) {
                delay(pollInterval)
                fetch()
            }
        }
    }

    companion object {
        const val POLL_INTERVAL_MS = 3_000L
    }
}

/** «Leer completo» (P7): solo mientras nadie lo ha pedido. Va entre las acciones de la pantalla. */
@Composable
internal fun FullTextAction(state: FullTextState, onReadFull: () -> Unit, modifier: Modifier = Modifier) {
    if (state.state == FullPaperState.NotRequested) {
        OutlineAction("Leer completo", Icons.AutoMirrored.Filled.MenuBook, modifier, onClick = onReadFull)
    }
}

/**
 * La sección «Texto completo», debajo del resumen o del artículo: el texto que llega,
 * la nota de «no hay versión» o el fallo con «Reintentar».
 */
internal fun LazyListScope.fullTextSection(state: FullTextState, onReadFull: () -> Unit) {
    when (state.state) {
        FullPaperState.Pending, FullPaperState.Done -> {
            item(key = "full-head") {
                Text(
                    "TEXTO COMPLETO",
                    fontSize = 12.sp,
                    letterSpacing = 0.6.sp,
                    color = Atalaya.colors.text3,
                    modifier = Modifier.padding(top = 28.dp, bottom = 14.dp),
                )
            }
            // La clave es la posición: al llegar más trozos, los que ya se leen no se recrean ni saltan.
            itemsIndexed(state.blocks, key = { i, _ -> "full-$i" }) { _, block ->
                ArticleBlock(block, muted = false)
            }
        }
        FullPaperState.NotAvailable -> item(key = "full-missing") {
            FullNote("arXiv no tiene una versión completa de este paper. Quedan el resumen y el PDF.", Modifier.padding(top = 24.dp))
        }
        FullPaperState.Failed -> item(key = "full-failed") {
            Column(Modifier.padding(top = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                FullNote("No se pudo traducir el paper completo.")
                OutlinedButton(onClick = onReadFull) { Text("Reintentar") }
            }
        }
        FullPaperState.NotRequested -> Unit
    }
}

@Composable
internal fun FullNote(text: String, modifier: Modifier = Modifier) {
    Text(text, fontSize = 15.sp, lineHeight = 22.sp, color = Atalaya.colors.text2, modifier = modifier)
}

/** Botón con borde y icono, de la fila de acciones. */
@Composable
internal fun OutlineAction(label: String, icon: ImageVector, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val c = Atalaya.colors
    Row(
        modifier
            .height(42.dp)
            .border(1.dp, c.divider, RoundedCornerShape(21.dp))
            .clip(RoundedCornerShape(21.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, tint = c.text, modifier = Modifier.size(18.dp))
        Text(label, fontSize = 14.sp, color = c.text)
    }
}
