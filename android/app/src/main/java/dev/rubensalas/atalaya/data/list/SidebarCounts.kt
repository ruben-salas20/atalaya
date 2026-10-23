package dev.rubensalas.atalaya.data.list

import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.Sidebar

/** Suma o resta a los números del menú lateral lo que cambió en un artículo de [feedId]. */
fun Sidebar.adjusted(feedId: Id, unreadDelta: Int, starredDelta: Int): Sidebar {
    fun Long.plus(d: Int) = (this + d).coerceAtLeast(0)
    return copy(
        unread = unread.plus(unreadDelta),
        starred = starred.plus(starredDelta),
        categories = categories.map { cat ->
            if (cat.feeds.none { it.id == feedId }) cat
            else cat.copy(
                unread = cat.unread.plus(unreadDelta),
                feeds = cat.feeds.map { if (it.id == feedId) it.copy(unread = it.unread.plus(unreadDelta)) else it },
            )
        },
    )
}
