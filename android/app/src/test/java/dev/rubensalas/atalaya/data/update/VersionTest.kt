package dev.rubensalas.atalaya.data.update

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class VersionTest {
    @Test
    fun readsTagWithOrWithoutV() {
        assertEquals(Version(0, 2, 1), Version.parse("v0.2.1"))
        assertEquals(Version(0, 2, 1), Version.parse("0.2.1"))
    }

    @Test
    fun ignoresSuffixAfterDash() {
        assertEquals(Version(0, 2, 1), Version.parse("v0.2.1-rc.1"))
        assertEquals(Version(0, 2, 0), Version.parse("0.2.0-beta"))
    }

    @Test
    fun rejectsWhatIsNotNumericXYZ() {
        assertNull(Version.parse("0.2"))
        assertNull(Version.parse("1.2.3.4"))
        assertNull(Version.parse("v0.2.x"))
        assertNull(Version.parse("latest"))
        assertNull(Version.parse(""))
        assertNull(Version.parse("-0.2.1"))
        assertNull(Version.parse("99999999999.0.0"))
    }

    @Test
    fun comparesByNumberNotByText() {
        assertTrue(Version(0, 10, 0) > Version(0, 9, 0))
        assertTrue(Version(0, 2, 1) > Version(0, 2, 0))
        assertTrue(Version(1, 0, 0) > Version(0, 99, 99))
        assertFalse(Version(0, 2, 0) > Version(0, 2, 0))
        assertFalse(Version(0, 2, 0) > Version(0, 2, 1))
    }

    @Test
    fun suffixedTagIsNotNewerThanSameNumber() {
        // Una etiqueta con sufijo es la misma versión numérica: no ofrece actualización.
        assertFalse(Version.parse("v0.2.0-beta")!! > Version.parse("0.2.0")!!)
    }
}
