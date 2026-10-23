package dev.rubensalas.atalaya.ui.settings

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.rubensalas.atalaya.AppContainer
import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.settings.Settings
import dev.rubensalas.atalaya.data.settings.SwipeAction
import dev.rubensalas.atalaya.ui.relativeTime
import dev.rubensalas.atalaya.ui.theme.Atalaya
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Resultado del botón «Probar». */
private sealed interface ProbeState {
    data object Idle : ProbeState
    data object Running : ProbeState
    data class Failed(val message: String) : ProbeState
}

private enum class EditDialog { Url, Key, SwipeLeft, SwipeRight }

/** Pantalla 5 de las maquetas (S3). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(container: AppContainer, onBack: () -> Unit) {
    val repo = container.settings
    val settings by repo.settings.collectAsStateWithLifecycle(initialValue = null)
    val scope = rememberCoroutineScope()
    var probe by remember { mutableStateOf<ProbeState>(ProbeState.Idle) }
    var dialog by remember { mutableStateOf<EditDialog?>(null) }

    // «hace 5 min» se refresca solo mientras la pantalla está abierta.
    var tick by remember { mutableStateOf(0) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000)
            tick++
        }
    }

    fun runProbe() {
        probe = ProbeState.Running
        scope.launch {
            probe = try {
                val status = container.client.status()
                if (status.miniflux) {
                    repo.markConnected()
                    ProbeState.Idle
                } else {
                    ProbeState.Failed("El servicio responde, pero no llega a Miniflux")
                }
            } catch (e: ApiException) {
                ProbeState.Failed(if (e.status == 401) "La clave no es correcta" else e.message.orEmpty())
            }
        }
    }

    // Si Ajustes es la primera pantalla (sin clave), «atrás» lleva a la lista en vez de cerrar la app.
    BackHandler(onBack = onBack)

    Scaffold(
        topBar = {
            Column {
                TopAppBar(
                    title = { Text("Ajustes", fontSize = 20.sp, fontWeight = FontWeight.Medium) },
                    navigationIcon = {
                        IconButton(onClick = onBack) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Volver")
                        }
                    },
                    colors = TopAppBarDefaults.topAppBarColors(containerColor = Atalaya.colors.bg),
                )
                HorizontalDivider(color = Atalaya.colors.divider)
            }
        },
        containerColor = Atalaya.colors.bg,
    ) { padding ->
        val s = settings ?: return@Scaffold
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState()),
        ) {
            SectionHead("Servicio Atalaya")
            SettingItem("Dirección", s.serviceUrl, onClick = { dialog = EditDialog.Url })
            SettingItem(
                "Clave de la API",
                if (s.apiKey.isBlank()) "Sin clave" else "•".repeat(12),
                onClick = { dialog = EditDialog.Key },
            )
            SettingItem(
                title = "Estado",
                subtitle = statusText(s, probe, tick),
                dot = true,
                trailing = {
                    OutlinedButton(
                        onClick = ::runProbe,
                        enabled = probe != ProbeState.Running,
                        shape = CircleShape,
                        contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 12.dp),
                        modifier = Modifier.height(32.dp),
                    ) { Text("Probar", fontSize = 13.5.sp, fontWeight = FontWeight.Medium) }
                },
            )
            Separator()

            SectionHead("Lectura")
            SettingItem("Tamaño de letra", null, minHeight = 44)
            FontScaleSlider(s.fontScale) { scope.launch { repo.setFontScale(it) } }
            Separator()

            SectionHead("Gestos")
            SettingItem("Deslizar a la izquierda", s.swipeLeft.label, onClick = { dialog = EditDialog.SwipeLeft })
            SettingItem("Deslizar a la derecha", s.swipeRight.label, onClick = { dialog = EditDialog.SwipeRight })
        }

        when (dialog) {
            EditDialog.Url -> TextDialog(
                title = "Dirección del servicio",
                initial = s.serviceUrl,
                keyboardType = KeyboardType.Uri,
                onDismiss = { dialog = null },
                onSave = {
                    scope.launch { repo.setServiceUrl(it) }
                    probe = ProbeState.Idle
                    dialog = null
                },
            )
            EditDialog.Key -> TextDialog(
                title = "Clave de la API",
                initial = s.apiKey,
                keyboardType = KeyboardType.Password,
                secret = true,
                onDismiss = { dialog = null },
                onSave = {
                    scope.launch { repo.setApiKey(it) }
                    probe = ProbeState.Idle
                    dialog = null
                },
            )
            EditDialog.SwipeLeft -> SwipeDialog("Deslizar a la izquierda", s.swipeLeft, { dialog = null }) {
                scope.launch { repo.setSwipeLeft(it) }
                dialog = null
            }
            EditDialog.SwipeRight -> SwipeDialog("Deslizar a la derecha", s.swipeRight, { dialog = null }) {
                scope.launch { repo.setSwipeRight(it) }
                dialog = null
            }
            null -> Unit
        }
    }
}

@Suppress("UNUSED_PARAMETER")
private fun statusText(s: Settings, probe: ProbeState, tick: Int): String = when (probe) {
    ProbeState.Running -> "Probando…"
    is ProbeState.Failed -> probe.message
    ProbeState.Idle -> when {
        s.apiKey.isBlank() -> "Falta la clave"
        s.lastConnectedAt == null -> "Sin probar"
        else -> "Conectado · ${relativeTime(s.lastConnectedAt)}"
    }
}

@Composable
private fun SectionHead(text: String) {
    Text(
        text.uppercase(),
        fontSize = 12.5.sp,
        letterSpacing = 0.5.sp,
        color = Atalaya.colors.text3,
        modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 18.dp, bottom = 6.dp),
    )
}

@Composable
private fun Separator() {
    HorizontalDivider(color = Atalaya.colors.divider, modifier = Modifier.padding(vertical = 6.dp))
}

@Composable
private fun SettingItem(
    title: String,
    subtitle: String?,
    onClick: (() -> Unit)? = null,
    dot: Boolean = false,
    minHeight: Int = 56,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = minHeight.dp)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 20.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, fontSize = 15.5.sp, color = Atalaya.colors.text)
            if (subtitle != null) {
                Row(Modifier.padding(top = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                    if (dot) {
                        Box(Modifier.size(7.dp).background(Atalaya.colors.text2, CircleShape))
                        Spacer(Modifier.width(6.dp))
                    }
                    Text(subtitle, fontSize = 13.sp, color = Atalaya.colors.text2)
                }
            }
        }
        trailing?.invoke()
    }
}

@Composable
private fun FontScaleSlider(value: Float, onChange: (Float) -> Unit) {
    // El valor se guarda al soltar, para no escribir en disco en cada píxel.
    var current by remember(value) { mutableFloatStateOf(value) }
    Row(
        Modifier.padding(start = 20.dp, end = 20.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text("A", fontSize = 13.sp)
        Slider(
            value = current,
            onValueChange = { current = it },
            onValueChangeFinished = { onChange(current) },
            valueRange = Settings.MIN_FONT_SCALE..Settings.MAX_FONT_SCALE,
            colors = SliderDefaults.colors(
                thumbColor = Atalaya.colors.accent,
                activeTrackColor = Atalaya.colors.accent,
                inactiveTrackColor = Atalaya.colors.surface2,
            ),
            modifier = Modifier.weight(1f),
        )
        Text("A", fontSize = 20.sp)
    }
}

@Composable
private fun TextDialog(
    title: String,
    initial: String,
    keyboardType: KeyboardType,
    secret: Boolean = false,
    onDismiss: () -> Unit,
    onSave: (String) -> Unit,
) {
    var text by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                singleLine = true,
                visualTransformation = if (secret) PasswordVisualTransformation() else androidx.compose.ui.text.input.VisualTransformation.None,
                keyboardOptions = KeyboardOptions(keyboardType = keyboardType),
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = { TextButton(onClick = { onSave(text) }) { Text("Guardar") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancelar") } },
    )
}

@Composable
private fun SwipeDialog(title: String, current: SwipeAction, onDismiss: () -> Unit, onPick: (SwipeAction) -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column {
                SwipeAction.entries.forEach { action ->
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .selectable(selected = action == current, role = Role.RadioButton) { onPick(action) }
                            .padding(vertical = 8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = action == current, onClick = null)
                        Spacer(Modifier.width(12.dp))
                        Text(action.label)
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Cerrar") } },
    )
}
