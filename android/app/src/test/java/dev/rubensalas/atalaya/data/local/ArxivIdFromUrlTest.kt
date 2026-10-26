package dev.rubensalas.atalaya.data.local

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ArxivIdFromUrlTest {
    @Test
    fun readsTheIdFromArxivAddresses() {
        assertEquals("2610.08812", arxivIdFromUrl("https://arxiv.org/abs/2610.08812v2"))
        assertEquals("2610.08812", arxivIdFromUrl("http://arxiv.org/pdf/2610.08812.pdf"))
        assertEquals("1706.03762", arxivIdFromUrl("https://arxiv.org/html/1706.03762v7/"))
        assertEquals("hep-th/9901001", arxivIdFromUrl("https://arxiv.org/abs/hep-th/9901001"))
        assertNull(arxivIdFromUrl("https://example.com/abs/2610.08812"))
        assertNull(arxivIdFromUrl("https://arxiv.org/list/cs.CL/new"))
    }
}
