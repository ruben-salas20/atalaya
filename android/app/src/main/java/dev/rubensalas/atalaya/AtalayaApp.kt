package dev.rubensalas.atalaya

import android.app.Application
import dev.rubensalas.atalaya.data.api.AtalayaClient
import dev.rubensalas.atalaya.data.list.ListRepository
import dev.rubensalas.atalaya.data.list.RemoteListRepository
import dev.rubensalas.atalaya.data.settings.SettingsRepository

/** Las piezas compartidas de la app, creadas una vez. Sin biblioteca de inyección. */
class AppContainer(app: Application) {
    val settings = SettingsRepository(app)
    val client = AtalayaClient(config = { settings.current().serverConfig })
    val list: ListRepository = RemoteListRepository(client, settings)
}

class AtalayaApp : Application() {
    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
    }
}
