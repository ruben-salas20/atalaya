package dev.rubensalas.atalaya.ui.settings

import dev.rubensalas.atalaya.data.api.ApiException
import dev.rubensalas.atalaya.data.api.Cleanup
import dev.rubensalas.atalaya.data.api.Entry
import dev.rubensalas.atalaya.data.api.EntryPage
import dev.rubensalas.atalaya.data.api.EntryQuery
import dev.rubensalas.atalaya.data.api.EntrySummary
import dev.rubensalas.atalaya.data.api.EntryUpdate
import dev.rubensalas.atalaya.data.api.Id
import dev.rubensalas.atalaya.data.api.MarkAllReadQuery
import dev.rubensalas.atalaya.data.api.Sidebar
import dev.rubensalas.atalaya.data.list.ListRepository
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class TranslationCleanupViewModelTest {
    private class FakeList : ListRepository {
        var preview = Cleanup(entries = 0, papers = 0, bytes = 0)
        var failPreview: ApiException? = null
        var done = Cleanup(entries = 0, papers = 0, bytes = 0)
        var failCleanup: ApiException? = null
        var previews = 0
        var cleanups = 0

        override suspend fun translationCleanupPreview(): Cleanup {
            previews++
            failPreview?.let { throw it }
            return preview
        }

        override suspend fun cleanupTranslations(): Cleanup {
            cleanups++
            failCleanup?.let { throw it }
            return done
        }

        override suspend fun sidebar(): Sidebar = throw UnsupportedOperationException()
        override suspend fun entries(query: EntryQuery): EntryPage = throw UnsupportedOperationException()
        override suspend fun entry(id: Id): Entry = throw UnsupportedOperationException()
        override suspend fun updateEntry(id: Id, update: EntryUpdate): EntrySummary = throw UnsupportedOperationException()
        override suspend fun markAllRead(scope: MarkAllReadQuery) = throw UnsupportedOperationException()
        override suspend fun markSynced() = throw UnsupportedOperationException()
        override suspend fun deleteTranslation(id: Id) = throw UnsupportedOperationException()
    }

    private val dispatcher = StandardTestDispatcher()
    private val scope = TestScope(dispatcher)
    private val list = FakeList()

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun requestAsksForThePreviewAndOpensTheConfirmationWithoutDeleting() = with(scope) {
        list.preview = Cleanup(entries = 3, papers = 2, bytes = 1_500_000)
        val vm = TranslationCleanupViewModel(list)
        vm.request()
        runCurrent()
        assertEquals(1, list.previews)
        assertEquals(0, list.cleanups)
        assertEquals(Cleanup(3, 2, 1_500_000), vm.state.value.preview)
        assertFalse(vm.state.value.busy)
    }

    @Test
    fun nothingToDeleteSaysSoAndOpensNoDialog() = with(scope) {
        val vm = TranslationCleanupViewModel(list)
        vm.request()
        runCurrent()
        assertNull(vm.state.value.preview)
        assertEquals("No hay traducciones de lo leído que borrar", vm.state.value.message)
        assertEquals(0, list.cleanups)
    }

    @Test
    fun cancelingTheDialogDeletesNothing() = with(scope) {
        list.preview = Cleanup(entries = 3, papers = 2, bytes = 1_500_000)
        val vm = TranslationCleanupViewModel(list)
        vm.request()
        runCurrent()
        vm.dismiss()
        assertNull(vm.state.value.preview)
        assertEquals(0, list.cleanups)
    }

    @Test
    fun confirmingDeletesAndSaysHowMuchWasFreed() = with(scope) {
        list.preview = Cleanup(entries = 3, papers = 2, bytes = 1_500_000)
        list.done = Cleanup(entries = 3, papers = 2, bytes = 1_500_000)
        val vm = TranslationCleanupViewModel(list)
        vm.request()
        runCurrent()
        vm.confirm()
        runCurrent()
        assertEquals(1, list.cleanups)
        assertNull(vm.state.value.preview)
        assertEquals("Liberados 1,5 MB", vm.state.value.message)
        vm.dismissMessage()
        assertNull(vm.state.value.message)
    }

    @Test
    fun failedPreviewShowsTheServiceMessage() = with(scope) {
        list.failPreview = ApiException(null, "No se pudo conectar con el servicio")
        val vm = TranslationCleanupViewModel(list)
        vm.request()
        runCurrent()
        assertNull(vm.state.value.preview)
        assertEquals("No se pudo conectar con el servicio", vm.state.value.message)
        assertFalse(vm.state.value.busy)
    }

    @Test
    fun failedCleanupShowsTheMessageAndDoesNotClaimSpace() = with(scope) {
        list.preview = Cleanup(entries = 3, papers = 2, bytes = 1_500_000)
        list.failCleanup = ApiException(500, "Algo salió mal")
        val vm = TranslationCleanupViewModel(list)
        vm.request()
        runCurrent()
        vm.confirm()
        runCurrent()
        assertEquals("Algo salió mal", vm.state.value.message)
        assertFalse(vm.state.value.busy)
    }

    @Test
    fun dialogTextSaysWhatGoesAndWhatStays() {
        assertEquals(
            "Se borrarán las traducciones de 3 artículos leídos y 2 papers (1,5 MB). " +
                "Se vuelven a traducir si los abres. No se borra nada de Miniflux ni de Guardados.",
            cleanupQuestion(Cleanup(entries = 3, papers = 2, bytes = 1_500_000)),
        )
    }

    @Test
    fun megabytesHaveOneDecimalAndAComma() {
        assertEquals("0,0 MB", megabytesText(0))
        assertEquals("0,7 MB", megabytesText(700_000))
        assertEquals("1,5 MB", megabytesText(1_500_000))
        assertEquals("12,3 MB", megabytesText(12_345_678))
    }
}
