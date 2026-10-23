package dev.rubensalas.atalaya.ui.list

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.ui.theme.Atalaya
import java.time.Instant

/** Una fila compacta de la lista (L2). Si está leída, va en gris (L3). */
@Composable
fun EntryRow(entry: EntrySummary, now: Instant, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = Atalaya.colors
    val metaColor = if (entry.read) c.text3 else c.text2
    Column(modifier.fillMaxWidth().clickable(onClick = onClick)) {
        Column(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                entry.title,
                fontSize = 15.5.sp,
                lineHeight = 21.sp,
                fontWeight = if (entry.read) FontWeight.Normal else FontWeight.Medium,
                color = if (entry.read) c.text3 else c.text,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(
                    "${entry.feedTitle} · ${shortAge(entry.publishedAt, now)}",
                    fontSize = 13.sp,
                    color = metaColor,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (entry.starred) {
                    Icon(Icons.Filled.Bookmark, contentDescription = "Guardado", tint = metaColor, modifier = Modifier.size(14.dp))
                }
                Spacer(Modifier.weight(1f))
                langBadge(entry.lang)?.let { LangBadge(it) }
            }
        }
        HorizontalDivider(thickness = 1.dp, color = c.divider)
    }
}

@Composable
private fun LangBadge(text: String) {
    val c = Atalaya.colors
    Text(
        text,
        fontSize = 10.5.sp,
        lineHeight = 15.sp,
        fontWeight = FontWeight.Medium,
        letterSpacing = 0.5.sp,
        color = c.text3,
        modifier = Modifier
            .border(1.dp, c.text3, RoundedCornerShape(4.dp))
            .padding(horizontal = 4.dp),
    )
}
