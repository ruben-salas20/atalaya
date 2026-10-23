package dev.rubensalas.atalaya.ui.list

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Adjust
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.RssFeed
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.outlined.BookmarkBorder
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.api.View
import dev.rubensalas.atalaya.ui.relativeTime
import dev.rubensalas.atalaya.ui.theme.Atalaya
import java.time.Instant

/** Nombre de cada vista fija, tal como sale en el menú y en la barra. */
fun View.label(): String = when (this) {
    View.All -> "Todos"
    View.Unread -> "Sin leer"
    View.Starred -> "Guardados"
}

fun ListSelection.title(): String = when (this) {
    is ListSelection.Fixed -> view.label()
    is ListSelection.Category -> title
    is ListSelection.Feed -> title
}

/** «Sincronizado (5 min)» (S2). */
fun syncLabel(lastSyncAt: Instant?, now: Instant): String =
    if (lastSyncAt == null) "Sin sincronizar"
    else "Sincronizado (${relativeTime(lastSyncAt, now).removePrefix("hace ")})"

/** Menú lateral (pantalla 2): vistas fijas, «Buscar papers», carpetas con sus feeds, Feeds, Ajustes y la sincronización. */
@Composable
fun ListDrawer(
    sidebar: Sidebar?,
    selection: ListSelection,
    expanded: Set<Long>,
    lastSyncAt: Instant?,
    now: Instant,
    onSelect: (ListSelection) -> Unit,
    onToggleCategory: (Long) -> Unit,
    onOpenFeeds: () -> Unit,
    onOpenSettings: () -> Unit,
    onOpenPapers: () -> Unit,
) {
    val c = Atalaya.colors
    ModalDrawerSheet(
        modifier = Modifier.width(296.dp),
        drawerShape = RectangleShape,
        drawerContainerColor = c.bg,
        drawerContentColor = c.text,
    ) {
        Column(Modifier.fillMaxHeight().padding(vertical = 8.dp)) {
            Text(
                "Atalaya",
                fontSize = 22.sp,
                fontWeight = FontWeight.Medium,
                modifier = Modifier.padding(start = 24.dp, end = 24.dp, top = 12.dp, bottom = 10.dp),
            )
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
                val fixed = listOf(
                    Triple(View.All, Icons.AutoMirrored.Filled.List, sidebar?.total),
                    Triple(View.Unread, Icons.Filled.Adjust, sidebar?.unread),
                    Triple(View.Starred, Icons.Outlined.BookmarkBorder, sidebar?.starred),
                )
                for ((view, icon, count) in fixed) {
                    val sel = ListSelection.Fixed(view)
                    DrawerItem(view.label(), icon = icon, count = count, active = selection == sel, onClick = { onSelect(sel) })
                }
                DrawerItem("Buscar papers", icon = Icons.Filled.Search, onClick = onOpenPapers)
                val categories = sidebar?.categories.orEmpty()
                if (categories.isNotEmpty()) {
                    DrawerSeparator()
                    Text(
                        "CARPETAS",
                        fontSize = 12.5.sp,
                        letterSpacing = 0.5.sp,
                        color = c.text3,
                        modifier = Modifier.padding(horizontal = 30.dp, vertical = 4.dp),
                    )
                }
                for (cat in categories) {
                    val open = cat.id in expanded
                    DrawerItem(
                        cat.title,
                        icon = if (open) Icons.Filled.ExpandMore else Icons.Filled.ChevronRight,
                        iconDescription = if (open) "Plegar" else "Desplegar",
                        onIconClick = { onToggleCategory(cat.id) },
                        count = cat.unread,
                        active = selection is ListSelection.Category && selection.id == cat.id,
                        onClick = { onSelect(ListSelection.Category(cat.id, cat.title)) },
                    )
                    if (open) {
                        for (feed in cat.feeds) {
                            DrawerItem(
                                feed.title,
                                count = feed.unread,
                                sub = true,
                                active = selection is ListSelection.Feed && selection.id == feed.id,
                                onClick = { onSelect(ListSelection.Feed(feed.id, feed.title)) },
                            )
                        }
                    }
                }
            }
            HorizontalDivider(thickness = 1.dp, color = c.divider)
            Spacer(Modifier.height(4.dp))
            DrawerItem("Feeds", icon = Icons.Filled.RssFeed, onClick = onOpenFeeds)
            DrawerItem("Ajustes", icon = Icons.Filled.Tune, onClick = onOpenSettings)
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier.padding(start = 30.dp, end = 30.dp, top = 6.dp, bottom = 10.dp),
            ) {
                Box(Modifier.size(7.dp).clip(CircleShape).background(c.text2))
                Text(syncLabel(lastSyncAt, now), fontSize = 12.5.sp, color = c.text3)
            }
        }
    }
}

@Composable
private fun DrawerItem(
    label: String,
    onClick: () -> Unit,
    icon: ImageVector? = null,
    iconDescription: String? = null,
    onIconClick: (() -> Unit)? = null,
    count: Long? = null,
    sub: Boolean = false,
    active: Boolean = false,
) {
    val c = Atalaya.colors
    val height: Dp = if (sub) 36.dp else 40.dp
    val fontSize: TextUnit = if (sub) 14.5.sp else 15.sp
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .padding(horizontal = 10.dp)
            .fillMaxWidth()
            .height(height)
            .clip(RoundedCornerShape(24.dp))
            .background(if (active) c.surface2 else c.bg)
            .clickable(onClick = onClick)
            .padding(start = if (sub) 58.dp else 20.dp, end = 16.dp),
    ) {
        if (icon != null) {
            val iconModifier = if (onIconClick != null) {
                Modifier.clip(CircleShape).clickable(onClick = onIconClick)
            } else {
                Modifier
            }
            Icon(icon, contentDescription = iconDescription, tint = c.text2, modifier = iconModifier.size(24.dp))
            Spacer(Modifier.width(14.dp))
        }
        Text(
            label,
            fontSize = fontSize,
            fontWeight = if (active) FontWeight.Medium else FontWeight.Normal,
            color = if (sub && !active) c.text2 else c.text,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (count != null) {
            Text(count.toString(), fontSize = 13.sp, color = c.text2, modifier = Modifier.padding(start = 8.dp))
        }
    }
}

@Composable
private fun DrawerSeparator() {
    HorizontalDivider(
        thickness = 1.dp,
        color = Atalaya.colors.divider,
        modifier = Modifier.padding(horizontal = 24.dp, vertical = 6.dp),
    )
}
