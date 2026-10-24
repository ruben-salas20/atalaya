package dev.rubensalas.atalaya.ui.list

import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.View
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ListSelectionTest {
    @Test
    fun blankSearchIsTheNormalList() {
        assertNull(ListSelection.Fixed(View.Unread).query(50, 0, "  ").q)
        assertNull(ListSelection.Fixed(View.Unread).query(50, 0).q)
    }

    @Test
    fun searchKeepsTheViewOfTheSelection() {
        assertEquals(
            EntryQuery(view = View.All, categoryId = 3, limit = 20, offset = 0, q = "modelos"),
            ListSelection.Category(3, "IA").query(20, 0, "modelos"),
        )
    }
}
