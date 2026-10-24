package dev.rubensalas.atalaya.data.list

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SearchTest {
    @Test
    fun foldDropsCaseAndAccentsLikeTheService() {
        assertEquals("modelos", foldForSearch("MODELÓS"))
        assertEquals("ano", foldForSearch("Año"))
        assertEquals("nandu", foldForSearch("Ñandú"))
        assertEquals("o", foldForSearch("ó"))
    }

    @Test
    fun foldLeavesLettersOutsideTheServiceTableAlone() {
        // Normalizer las plegaría; el servicio no, y la app tiene que dar lo mismo.
        assertEquals("ő", foldForSearch("ő"))
    }

    @Test
    fun blankSearchHasNoNeedle() {
        assertNull(searchNeedle(null))
        assertNull(searchNeedle("   "))
        assertEquals("ano", searchNeedle(" Año "))
    }

    @Test
    fun titleMatchesAnyOfTheTitlesWithoutAccentsOrCase() {
        assertTrue(titleMatches("ano", "Año nuevo"))
        assertTrue(titleMatches("ano", null, "Año"))
        assertFalse(titleMatches("ano", null, "Otra cosa"))
    }
}
