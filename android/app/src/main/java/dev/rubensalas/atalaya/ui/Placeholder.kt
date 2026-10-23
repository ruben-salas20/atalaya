package dev.rubensalas.atalaya.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.rubensalas.atalaya.ui.theme.Atalaya

/** Pantalla vacía para lo que aún no está hecho. Se borra cuando no la use nadie. */
@Composable
fun Placeholder(text: String, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        Text(text, color = Atalaya.colors.text3)
    }
}
