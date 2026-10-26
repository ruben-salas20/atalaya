package dev.rubensalas.atalaya

import android.app.Application
import coil3.ImageLoader
import coil3.PlatformContext
import coil3.SingletonImageLoader
import coil3.svg.SvgDecoder
import dev.rubensalas.atalaya.data.api.AtalayaClient
import dev.rubensalas.atalaya.data.formulas.FormulaSvgFetcherFactory
import dev.rubensalas.atalaya.data.formulas.FormulaSvgKeyer
import dev.rubensalas.atalaya.data.update.ReleaseClient
import dev.rubensalas.atalaya.data.update.UpdateChecker
import dev.rubensalas.atalaya.data.update.UpdateRepository
import dev.rubensalas.atalaya.data.update.Version
import dev.rubensalas.atalaya.data.list.ListRepository
import dev.rubensalas.atalaya.data.list.OfflineListRepository
import dev.rubensalas.atalaya.data.local.LocalDatabase
import dev.rubensalas.atalaya.data.papers.PapersRepository
import dev.rubensalas.atalaya.data.papers.RecentSearches
import dev.rubensalas.atalaya.data.papers.StoredRecentSearches
import dev.rubensalas.atalaya.data.settings.SettingsRepository
import dev.rubensalas.atalaya.data.sync.Sync
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/** Las piezas compartidas de la app, creadas una vez. Sin biblioteca de inyección. */
class AppContainer(app: Application, scope: CoroutineScope) {
    val settings = SettingsRepository(app)
    val client = AtalayaClient(config = { settings.current().serverConfig })
    private val database = LocalDatabase.create(app)
    val offline = OfflineListRepository(
        client = client,
        dao = database.dao(),
        onSynced = { settings.markSynced() },
        requestPush = { Sync.syncNow(app) },
    )
    val list: ListRepository = offline
    val papers = PapersRepository(client, onSaved = { Sync.syncNow(app) })
    val recentSearches: RecentSearches = StoredRecentSearches(app)
    val updates = run {
        val releases = ReleaseClient(BuildConfig.RELEASES_API)
        val installed = Version.parse(BuildConfig.VERSION_NAME) ?: error("VERSION_NAME no es X.Y.Z: ${BuildConfig.VERSION_NAME}")
        UpdateRepository(app, settings, releases, UpdateChecker(releases, installed), scope)
    }
}

class AtalayaApp : Application(), SingletonImageLoader.Factory {
    lateinit var container: AppContainer
        private set

    // Las fórmulas del paper completo son SVG (ADR 0010); el resto de imágenes sigue como estaba.
    override fun newImageLoader(context: PlatformContext): ImageLoader = ImageLoader.Builder(context)
        .components {
            add(SvgDecoder.Factory())
            add(FormulaSvgKeyer)
            add(FormulaSvgFetcherFactory(AtalayaClient.defaultHttpClient()))
        }
        .build()

    override fun onCreate() {
        super.onCreate()
        // Las tareas largas (la descarga de una actualización) no mueren al cambiar de pantalla.
        container = AppContainer(this, CoroutineScope(SupervisorJob() + Dispatchers.Default))
        Sync.schedulePeriodic(this)
    }
}
