package dev.rubensalas.atalaya.ui.feeds

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextFieldColors
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.material3.TextButton
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.TextStyle
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import dev.rubensalas.atalaya.data.api.Category
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.ui.theme.Atalaya

// Piezas de las pantallas 9 a 11, con las clases CSS de las maquetas que copian.

/** `.appbar` con su línea inferior. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FeedsTopBar(
    title: String,
    navigationIcon: @Composable () -> Unit,
    actions: @Composable RowScope.() -> Unit = {},
) {
    Column {
        TopAppBar(
            title = {
                Text(title, fontSize = 20.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            },
            navigationIcon = navigationIcon,
            actions = actions,
            colors = TopAppBarDefaults.topAppBarColors(containerColor = Atalaya.colors.bg),
        )
        HorizontalDivider(color = Atalaya.colors.divider)
    }
}

/** `.fl`: la etiqueta encima de cada campo. */
@Composable
fun FieldLabel(text: String) {
    Text(text, fontSize = 12.5.sp, color = Atalaya.colors.text2, modifier = Modifier.padding(bottom = 6.dp))
}

/** `.help`. */
@Composable
fun HelpText(text: String, modifier: Modifier = Modifier) {
    Text(text, fontSize = 12.5.sp, lineHeight = 17.sp, color = Atalaya.colors.text3, modifier = modifier.padding(top = 6.dp))
}

/** Mensaje de error de una llamada: el texto del servicio, tal cual. */
@Composable
fun ErrorText(text: String, modifier: Modifier = Modifier) {
    Text(text, fontSize = 13.5.sp, lineHeight = 19.sp, color = Atalaya.colors.text, fontWeight = FontWeight.Medium, modifier = modifier)
}

val FieldShape = RoundedCornerShape(10.dp)

/** `.fi`: fondo `surface`, borde `divider` y borde `accent` con el foco. */
@Composable
fun fieldColors(): TextFieldColors = OutlinedTextFieldDefaults.colors(
    focusedContainerColor = Atalaya.colors.surface,
    unfocusedContainerColor = Atalaya.colors.surface,
    disabledContainerColor = Atalaya.colors.surface,
    focusedBorderColor = Atalaya.colors.accent,
    unfocusedBorderColor = Atalaya.colors.divider,
    disabledBorderColor = Atalaya.colors.divider,
    focusedTextColor = Atalaya.colors.text,
    unfocusedTextColor = Atalaya.colors.text,
    cursorColor = Atalaya.colors.accent,
)

@Composable
fun FormTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    trailingIcon: (@Composable () -> Unit)? = null,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        enabled = enabled,
        singleLine = true,
        shape = FieldShape,
        colors = fieldColors(),
        trailingIcon = trailingIcon,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        textStyle = TextStyle(fontSize = 15.sp),
        modifier = modifier.fillMaxWidth(),
    )
}

/**
 * Selector de carpeta: un `.fi` con flecha que abre un menú. Si se pasa
 * [onNewFolder], el menú acaba con «Nueva carpeta…».
 */
@Composable
fun FolderPicker(
    categories: List<Category>,
    selected: Id?,
    onSelect: (Id) -> Unit,
    enabled: Boolean = true,
    onNewFolder: (() -> Unit)? = null,
) {
    var open by remember { mutableStateOf(false) }
    val title = categories.firstOrNull { it.id == selected }?.title ?: "Elige una carpeta"
    Box {
        Row(
            Modifier
                .fillMaxWidth()
                .height(46.dp)
                .background(Atalaya.colors.surface, FieldShape)
                .border(1.dp, Atalaya.colors.divider, FieldShape)
                .clickable(enabled = enabled) { open = true }
                .padding(horizontal = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(title, fontSize = 15.sp, color = Atalaya.colors.text, modifier = Modifier.weight(1f))
            Icon(Icons.Filled.ExpandMore, contentDescription = null, tint = Atalaya.colors.text, modifier = Modifier.size(18.dp))
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            categories.forEach { c ->
                DropdownMenuItem(text = { Text(c.title) }, onClick = {
                    open = false
                    onSelect(c.id)
                })
            }
            if (onNewFolder != null) {
                DropdownMenuItem(text = { Text("Nueva carpeta…", color = Atalaya.colors.text2) }, onClick = {
                    open = false
                    onNewFolder()
                })
            }
        }
    }
}

/** `.row-toggle` de «Descargar el artículo completo» (F3, F4). */
@Composable
fun CrawlerToggle(checked: Boolean, onChange: (Boolean) -> Unit, enabled: Boolean = true) {
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(enabled = enabled) { onChange(!checked) },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text("Descargar el artículo completo", fontSize = 15.sp, color = Atalaya.colors.text)
            Text(
                "Para feeds que solo traen un resumen",
                fontSize = 12.5.sp,
                lineHeight = 17.sp,
                color = Atalaya.colors.text3,
                modifier = Modifier.padding(top = 2.dp),
            )
        }
        Switch(
            checked = checked,
            onCheckedChange = onChange,
            enabled = enabled,
            colors = SwitchDefaults.colors(
                checkedTrackColor = Atalaya.colors.accent,
                checkedThumbColor = Atalaya.colors.onAccent,
                checkedBorderColor = Color.Transparent,
                uncheckedTrackColor = Atalaya.colors.surface2,
                uncheckedThumbColor = Atalaya.colors.text3,
                uncheckedBorderColor = Color.Transparent,
            ),
        )
    }
}

/** `.primary`. */
@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, enabled: Boolean = true, busy: Boolean = false) {
    Button(
        onClick = onClick,
        enabled = enabled && !busy,
        shape = RoundedCornerShape(23.dp),
        colors = ButtonDefaults.buttonColors(
            containerColor = Atalaya.colors.accent,
            contentColor = Atalaya.colors.onAccent,
            disabledContainerColor = Atalaya.colors.surface2,
            disabledContentColor = Atalaya.colors.text3,
        ),
        modifier = Modifier.fillMaxWidth().height(46.dp),
    ) {
        if (busy) {
            CircularProgressIndicator(Modifier.size(18.dp), color = Atalaya.colors.text3, strokeWidth = 2.dp)
        } else {
            Text(text, fontSize = 15.sp, fontWeight = FontWeight.Medium)
        }
    }
}

/** `.danger`: borde `divider` y texto normal. */
@Composable
fun SecondaryButton(text: String, onClick: () -> Unit, enabled: Boolean = true) {
    OutlinedButton(
        onClick = onClick,
        enabled = enabled,
        shape = RoundedCornerShape(23.dp),
        border = BorderStroke(1.dp, Atalaya.colors.divider),
        colors = ButtonDefaults.outlinedButtonColors(contentColor = Atalaya.colors.text),
        modifier = Modifier.fillMaxWidth().height(46.dp),
    ) {
        Text(text, fontSize = 15.sp)
    }
}

/** `.stat`: una fila «nombre … valor» con línea inferior. */
@Composable
fun StatRow(label: String, value: String) {
    Column {
        Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(label, fontSize = 13.5.sp, color = Atalaya.colors.text2)
            Text(
                value,
                fontSize = 13.5.sp,
                color = Atalaya.colors.text,
                textAlign = TextAlign.End,
                modifier = Modifier.weight(1f),
            )
        }
        HorizontalDivider(color = Atalaya.colors.divider)
    }
}

@Composable
fun CenteredLoading() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        CircularProgressIndicator(color = Atalaya.colors.text3)
    }
}

/** Error al cargar una pantalla, con «Reintentar». */
@Composable
fun LoadError(message: String, onRetry: () -> Unit) {
    Column(
        Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(message, color = Atalaya.colors.text2, fontSize = 15.sp, textAlign = TextAlign.Center)
        TextButton(onClick = onRetry) { Text("Reintentar", color = Atalaya.colors.text) }
    }
}
