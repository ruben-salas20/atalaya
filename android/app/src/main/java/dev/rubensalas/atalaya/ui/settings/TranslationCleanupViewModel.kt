package dev.rubensalas.atalaya.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.Cleanup
import dev.rubensalas.atalaya.data.list.ListRepository
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.util.Locale

data class TranslationCleanupUiState(
    /** Con algo que borrar, el diálogo de confirmación; null si no hay diálogo. */
    val preview: Cleanup? = null,
    /** Se está pidiendo la cuenta o borrando. */
    val busy: Boolean = false,
    /** Aviso de abajo que se va solo. */
    val message: String? = null,
)

/** «Borrar traducciones de lo leído» en Ajustes (#91): primero cuenta, luego confirma y borra. */
class TranslationCleanupViewModel(private val list: ListRepository) : ViewModel() {
    private val _state = MutableStateFlow(TranslationCleanupUiState())
    val state: StateFlow<TranslationCleanupUiState> = _state.asStateFlow()

    /** Pide cuánto se borraría. Si no hay nada, lo dice; si hay algo, abre la confirmación. */
    fun request() {
        if (_state.value.busy) return
        _state.update { it.copy(busy = true) }
        viewModelScope.launch {
            try {
                val preview = list.translationCleanupPreview()
                _state.update {
                    if (preview.entries == 0L && preview.papers == 0L) {
                        it.copy(busy = false, message = "No hay traducciones de lo leído que borrar")
                    } else {
                        it.copy(busy = false, preview = preview)
                    }
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.update { it.copy(busy = false, message = e.message.orEmpty()) }
            }
        }
    }

    /** Cancelar la confirmación: no borra nada. */
    fun dismiss() = _state.update { it.copy(preview = null) }

    /** Confirmado: borra y dice cuánto se liberó. */
    fun confirm() {
        if (_state.value.busy) return
        _state.update { it.copy(preview = null, busy = true) }
        viewModelScope.launch {
            try {
                val done = list.cleanupTranslations()
                _state.update { it.copy(busy = false, message = "Liberados ${megabytesText(done.bytes)}") }
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.update { it.copy(busy = false, message = e.message.orEmpty()) }
            }
        }
    }

    fun dismissMessage() = _state.update { it.copy(message = null) }
}

/** La pregunta de la confirmación: cuántas traducciones y cuánto espacio. */
fun cleanupQuestion(c: Cleanup): String =
    "Se borrarán las traducciones de ${c.entries} artículos leídos y ${c.papers} papers (${megabytesText(c.bytes)}). " +
        "Se vuelven a traducir si los abres. No se borra nada de Miniflux ni de Guardados."

/** «12,3 MB»: los bytes en megabytes, con un decimal y coma, como se escribe en español. */
fun megabytesText(bytes: Long): String =
    String.format(Locale.forLanguageTag("es-ES"), "%.1f MB", bytes / 1_000_000.0)
