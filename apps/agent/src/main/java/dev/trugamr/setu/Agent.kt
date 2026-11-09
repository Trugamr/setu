package dev.trugamr.setu

import dev.trugamr.setu.protocol.StreamProtocol
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/** The capture server on this headset, as the agent talks to it. Calls block briefly. */
interface CaptureServer {
    /** The server's status, or null when no server answers. */
    fun status(): StreamProtocol.Status?

    /** Sends VBAN to [target], or stops sending it when null. */
    fun setVban(target: StreamProtocol.VbanTarget?)

    /** Asks the server to stop. */
    fun stop()
}

/** Where a server the agent launches writes its log, on the headset. */
const val SERVER_LOG_PATH = "/data/local/tmp/setu-server.log"

/**
 * This headset's adbd, reached with the agent's own adb key. Calls block, for up to a minute
 * while the headset asks the user something; [progress] says what the call waits for.
 */
interface HeadsetAdb {
    /**
     * Has the headset approve the agent's key: connects to adbd's TCP port, which makes the
     * headset ask, then returns adbd to USB. Setup opens the port first (`adb tcpip 5555`).
     */
    fun approve(progress: (String) -> Unit): AdbResult

    /** Launches the capture server detached from adbd, so it keeps running after adb disconnects. */
    fun launchServer(progress: (String) -> Unit): AdbResult
}

sealed interface AdbResult {
    data class Done(val message: String? = null) : AdbResult

    data class Failed(val message: String) : AdbResult
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

/** Something the user asked the agent to do: starting or stopping the server, or approving its key. */
sealed interface Operation {
    data class Working(val message: String) : Operation

    data class Done(val message: String) : Operation

    data class Failed(val message: String) : Operation
}

/** Everything the agent's screen shows. */
data class AgentState(
    val server: ServerState = ServerState.Checking,
    val vban: VbanSettings = VbanSettings(),
    /** The headset's Wi-Fi address, which a VBAN receiver filters on; null when offline. */
    val headsetAddress: String? = null,
    /** The latest operation, until it ends without a message; null when there is none. */
    val operation: Operation? = null,
)

/**
 * The agent's logic behind its screen. Once VBAN is configured, the agent owns the settings and
 * keeps the server in step with them: [run] reads the server's status every second and sends the
 * settings whenever the server reports something else, so a server that starts or restarts picks
 * them up. Until then, it takes over the target the server already has, e.g. one given to the
 * server on its command line, and leaves the server alone.
 *
 * The agent also starts and stops the server, one operation at a time, and has the headset
 * approve its adb key during setup.
 */
class Agent(
    private val server: CaptureServer,
    private val adb: HeadsetAdb,
    private val store: SettingsStore,
    private val headsetAddress: () -> String?,
    /** How long a launched server has to answer, and a stopped one to go quiet. */
    private val settleMs: Long = 5000,
) {
    private val saved = store.loadVban()
    private val mutableState = MutableStateFlow(AgentState(vban = saved ?: VbanSettings()))
    val state: StateFlow<AgentState> = mutableState.asStateFlow()

    /** Whether the VBAN settings are the agent's own: edited, or taken over from the server. */
    @Volatile
    private var configured = saved != null

    /** Wakes [run] early after an edit or an operation, so the screen and server catch up at once. */
    private val edited = Channel<Unit>(Channel.CONFLATED)

    private val operating = Mutex()

    fun editVban(settings: VbanSettings) {
        synchronized(this) {
            configured = true
            save(settings)
        }
        edited.trySend(Unit)
    }

    suspend fun startServer() = operate("Starting the capture server…") { progress ->
        when (val launched = adb.launchServer(progress)) {
            is AdbResult.Failed -> Operation.Failed(launched.message)
            is AdbResult.Done -> {
                progress("Waiting for the capture server…")
                if (settle(running = true)) null
                else Operation.Failed("The capture server didn't start. Its log is $SERVER_LOG_PATH on the headset.")
            }
        }
    }

    suspend fun stopServer() = operate("Stopping the capture server…") {
        server.stop()
        if (settle(running = false)) null else Operation.Failed("The capture server didn't stop.")
    }

    /** The last step of setup, which a PC asks for once it has opened adbd's TCP port. */
    suspend fun approve() = operate("Asking the headset to allow Setu…") { progress ->
        when (val approved = adb.approve(progress)) {
            is AdbResult.Failed -> Operation.Failed(approved.message)
            is AdbResult.Done -> approved.message?.let(Operation::Done)
        }
    }

    /**
     * Runs [block] unless another operation is running, showing its progress, then its outcome:
     * the operation it returns, or nothing.
     */
    private suspend fun operate(start: String, block: suspend (progress: (String) -> Unit) -> Operation?) {
        if (!operating.tryLock()) return
        try {
            withContext(Dispatchers.IO) {
                val progress = { message: String ->
                    mutableState.update { it.copy(operation = Operation.Working(message)) }
                }
                progress(start)
                val outcome = block(progress)
                mutableState.update { it.copy(operation = outcome) }
            }
        } finally {
            operating.unlock()
            edited.trySend(Unit)
        }
    }

    /** Waits up to [settleMs] for the server to be [running], or not. */
    private suspend fun settle(running: Boolean): Boolean {
        val deadline = System.nanoTime() + settleMs * 1_000_000
        while (true) {
            if ((server.status() != null) == running) return true
            if (System.nanoTime() >= deadline) return false
            delay(SETTLE_POLL_MS)
        }
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
        const val SETTLE_POLL_MS = 200L
    }
}
