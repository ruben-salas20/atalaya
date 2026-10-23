package dev.rubensalas.atalaya

import android.app.Application
import dev.rubensalas.atalaya.data.api.AtalayaClient
import dev.rubensalas.atalaya.data.list.ListRepository
import dev.rubensalas.atalaya.data.list.OfflineListRepository
import dev.rubensalas.atalaya.data.local.LocalDatabase
import dev.rubensalas.atalaya.data.papers.PapersRepository
import dev.rubensalas.atalaya.data.papers.RecentSearches
import dev.rubensalas.atalaya.data.papers.StoredRecentSearches
import dev.rubensalas.atalaya.data.settings.SettingsRepository
import dev.rubensalas.atalaya.data.sync.Sync

/** Las piezas compartidas de la app, creadas una vez. Sin biblioteca de inyección. */
class AppContainer(app: Application) {
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
}

class AtalayaApp : Application() {
    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
        Sync.schedulePeriodic(this)
    }
}
