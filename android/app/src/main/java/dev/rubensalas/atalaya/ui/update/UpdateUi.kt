package dev.rubensalas.atalaya.ui.update

import android.content.Context
import android.widget.Toast
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.rubensalas.atalaya.data.update.UpdateRepository
import dev.rubensalas.atalaya.data.update.UpdateState
import dev.rubensalas.atalaya.ui.theme.Atalaya

/**
 * Empieza la actualización. Si Android aún no deja instalar desde Atalaya,
 * explica por qué y abre su ajuste; al volver, [UpdateRepository.onResumed] sigue.
 */
fun startUpdate(context: Context, updates: UpdateRepository) {
    if (updates.install()) return
    Toast.makeText(
        context,
        "Android pide permiso para instalar apps desde Atalaya. Actívalo y vuelve: la actualización sigue sola.",
        Toast.LENGTH_LONG,
    ).show()
    context.startActivity(updates.unknownSourcesIntent())
}

/**
 * Aviso discreto de la lista (ADR 0012): la versión nueva, «Actualizar» y ✕.
 * Al tocar el texto, se ven las novedades.
 */
@Composable
fun UpdateBanner(state: UpdateState, onUpdate: () -> Unit, onClose: () -> Unit) {
    var showNotes by remember { mutableStateOf(false) }
    val c = Atalaya.colors
    val release = state.available
    if (release == null || state.bannerClosed) return

    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column(Modifier.weight(1f).clickable { showNotes = true }) {
            Text("Atalaya ${release.version} disponible", fontSize = 14.5.sp, color = c.text)
            val detail = when {
                state.busy -> "Preparando la actualización…"
                state.waitingPermission -> "Permite instalar desde Atalaya para seguir"
                state.error != null -> state.error
                else -> null
            }
            if (detail != null) Text(detail, fontSize = 12.5.sp, color = c.text2)
        }
        TextButton(onClick = onUpdate, enabled = !state.busy) {
            Text("Actualizar", fontSize = 14.sp, fontWeight = FontWeight.Medium)
        }
        IconButton(onClick = onClose) {
            Icon(Icons.Filled.Close, contentDescription = "Cerrar el aviso", tint = c.text2)
        }
    }

    if (showNotes) {
        ReleaseNotesDialog(
            version = release.version.toString(),
            notes = release.notes,
            onUpdate = {
                showNotes = false
                onUpdate()
            },
            onDismiss = { showNotes = false },
        )
    }
}

/** Las novedades de la versión, tal como las escribió la Release. */
@Composable
private fun ReleaseNotesDialog(version: String, notes: String, onUpdate: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Atalaya $version") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                Text(
                    notes.trim().ifEmpty { "Esta versión no trae novedades escritas." },
                    fontSize = 14.5.sp,
                    color = Atalaya.colors.text,
                )
            }
        },
        confirmButton = { TextButton(onClick = onUpdate) { Text("Actualizar") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cerrar") } },
    )
}
