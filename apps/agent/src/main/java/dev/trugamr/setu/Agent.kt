package dev.trugamr.setu

import dev.trugamr.setu.protocol.StreamProtocol
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/** The capture server on this headset, as the agent talks to it. Calls block briefly. */
interface CaptureServer {
    /** The server's status, or null when no server answers. */
    fun status(): StreamProtocol.Status?

    /** Sends VBAN to [target], or stops sending it when null. */
    fun setVban(target: StreamProtocol.VbanTarget?)
}

/** Where the agent keeps its settings across launches. */
interface SettingsStore {
    /** The saved VBAN settings, or null when they were never saved. */
    fun loadVban(): VbanSettings?

    fun saveVban(settings: VbanSettings)
}

sealed interface ServerState {
    data object Checking : ServerState

    data object NotRunning : ServerState

    data class Running(val status: StreamProtocol.Status) : ServerState
}

/** Everything the agent's screen shows. */
data class AgentState(
    val server: ServerState = ServerState.Checking,
    val vban: VbanSettings = VbanSettings(),
    /** The headset's Wi-Fi address, which a VBAN receiver filters on; null when offline. */
    val headsetAddress: String? = null,
)

/**
 * The agent's logic behind its screen. Once VBAN is configured, the agent owns the settings and
 * keeps the server in step with them: [run] reads the server's status every second and sends the
 * settings whenever the server reports something else, so a server that starts or restarts picks
 * them up. Until then, it takes over the target the server already has, e.g. one given to the
 * server on its command line, and leaves the server alone.
 */
class Agent(
    private val server: CaptureServer,
    private val store: SettingsStore,
    private val headsetAddress: () -> String?,
) {
    private val saved = store.loadVban()
    private val mutableState = MutableStateFlow(AgentState(vban = saved ?: VbanSettings()))
    val state: StateFlow<AgentState> = mutableState.asStateFlow()

    /** Whether the VBAN settings are the agent's own: edited, or taken over from the server. */
    @Volatile
    private var configured = saved != null

    /** Wakes [run] early after an edit, so a change reaches the server at once. */
    private val edited = Channel<Unit>(Channel.CONFLATED)

    fun editVban(settings: VbanSettings) {
        synchronized(this) {
            configured = true
            save(settings)
        }
        edited.trySend(Unit)
    }

    /** Refreshes until cancelled, e.g. while the screen is visible. */
    suspend fun run() {
        withContext(Dispatchers.IO) {
            while (true) {
                refresh()
                withTimeoutOrNull(POLL_MS) { edited.receive() }
            }
        }
    }

    /** One round: reads the server, brings its VBAN target in line with the settings, updates [state]. */
    fun refresh() {
        var status = server.status()
        if (status != null && !configured) {
            status.vban?.let(::takeOver)
        } else if (status != null) {
            val wanted = when (val validated = state.value.vban.validate()) {
                // Leave the server as it is until the settings are complete.
                is VbanSettings.Validated.Invalid -> status.vban
                is VbanSettings.Validated.Send -> validated.target
                VbanSettings.Validated.Off -> null
            }
            if (wanted != status.vban) {
                server.setVban(wanted)
                status = server.status() ?: status
            }
        }
        val serverState = if (status == null) ServerState.NotRunning else ServerState.Running(status)
        val address = headsetAddress()
        mutableState.update { it.copy(server = serverState, headsetAddress = address) }
    }

    private fun takeOver(target: StreamProtocol.VbanTarget) = synchronized(this) {
        if (!configured) {
            configured = true
            save(VbanSettings.of(target))
        }
    }

    private fun save(settings: VbanSettings) {
        store.saveVban(settings)
        mutableState.update { it.copy(vban = settings) }
    }

    private companion object {
        const val POLL_MS = 1000L
    }
}
