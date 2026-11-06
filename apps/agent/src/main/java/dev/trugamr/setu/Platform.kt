package dev.trugamr.setu

import android.content.Context
import android.net.ConnectivityManager
import java.net.Inet4Address

/** [SettingsStore] on SharedPreferences. */
class PreferencesStore(context: Context) : SettingsStore {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    override fun loadVban(): VbanSettings {
        val defaults = VbanSettings()
        return VbanSettings(
            enabled = prefs.getBoolean("vban.enabled", defaults.enabled),
            host = prefs.getString("vban.host", null) ?: defaults.host,
            port = prefs.getString("vban.port", null) ?: defaults.port,
            streamName = prefs.getString("vban.streamName", null) ?: defaults.streamName,
        )
    }

    override fun saveVban(settings: VbanSettings) {
        prefs.edit()
            .putBoolean("vban.enabled", settings.enabled)
            .putString("vban.host", settings.host)
            .putString("vban.port", settings.port)
            .putString("vban.streamName", settings.streamName)
            .apply()
    }
}

/** The headset's IPv4 address on its current network, or null when it has none. */
fun headsetAddress(context: Context): String? {
    val connectivity = context.getSystemService(ConnectivityManager::class.java) ?: return null
    val link = connectivity.getLinkProperties(connectivity.activeNetwork) ?: return null
    return link.linkAddresses
        .map { it.address }
        .firstOrNull { it is Inet4Address && !it.isLoopbackAddress }
        ?.hostAddress
}
