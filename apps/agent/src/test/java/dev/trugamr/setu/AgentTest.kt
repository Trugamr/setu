package dev.trugamr.setu

import dev.trugamr.setu.protocol.StreamProtocol
import java.net.InetSocketAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertNull
import kotlinx.coroutines.runBlocking
import org.junit.Test

class AgentTest {

    private val quest = StreamProtocol.VbanTarget(InetSocketAddress("192.168.1.20", 6980), "setu")
    private val sendToQuest = VbanSettings(enabled = true, host = "192.168.1.20")

    private class FakeServer : CaptureServer {
        var running = true
        var target: StreamProtocol.VbanTarget? = null
        val sets = mutableListOf<StreamProtocol.VbanTarget?>()

        override fun status() = if (running) StreamProtocol.Status(96, emptyList(), target) else null

        override fun setVban(target: StreamProtocol.VbanTarget?) {
            sets += target
            if (running) this.target = target
        }

        override fun stop() {
            running = false
        }
    }

    /** Launches [server] unless [failure] is set; [launches] counts the attempts. */
    private class FakeAdb(private val server: FakeServer) : HeadsetAdb {
        var failure: String? = null
        var startsServer = true
        var launches = 0
        val progress = mutableListOf<String>()

        override fun approve(progress: (String) -> Unit): AdbResult {
            progress("choose Always allow")
            this.progress += "choose Always allow"
            return failure?.let(AdbResult::Failed) ?: AdbResult.Done("allowed")
        }

        override fun launchServer(progress: (String) -> Unit): AdbResult {
            launches++
            failure?.let { return AdbResult.Failed(it) }
            if (startsServer) server.running = true
            return AdbResult.Done()
        }
    }

    private class MemoryStore(var vban: VbanSettings? = null) : SettingsStore {
        override fun loadVban() = vban

        override fun saveVban(settings: VbanSettings) {
            vban = settings
        }
    }

    private val server = FakeServer()
    private val adb = FakeAdb(server)
    private val store = MemoryStore()

    private fun agent() = Agent(server, adb, store, { "192.168.1.42" }, settleMs = 300)

    @Test
    fun sendsTheSavedTargetToARunningServer() {
        store.vban = sendToQuest
        val agent = agent()
        agent.refresh()
        assertEquals(quest, server.target)
        assertEquals(ServerState.Running(StreamProtocol.Status(96, emptyList(), quest)), agent.state.value.server)
        assertEquals("192.168.1.42", agent.state.value.headsetAddress)

        agent.refresh()
        assertEquals("sent once, while the server agrees", 1, server.sets.size)
    }

    @Test
    fun editsAreSavedAndTurningVbanOffClearsIt() {
        val agent = agent()
        agent.editVban(sendToQuest)
        agent.refresh()
        assertEquals(quest, server.target)

        agent.editVban(sendToQuest.copy(enabled = false))
        agent.refresh()
        assertNull(server.target)
        assertEquals(sendToQuest.copy(enabled = false), store.vban)
    }

    @Test
    fun incompleteSettingsLeaveTheServerAsItIs() {
        val agent = agent()
        agent.editVban(sendToQuest)
        agent.refresh()
        // Mid-edit: "192.168.1.20" on its way to "192.168.1.2".
        agent.editVban(sendToQuest.copy(host = "192.168.1."))
        agent.refresh()
        assertEquals(quest, server.target)
    }

    @Test
    fun takesOverTheServersTargetUntilConfigured() {
        server.target = quest
        val agent = agent()
        agent.refresh()
        assertEquals(quest, server.target)
        assertEquals(emptyList<StreamProtocol.VbanTarget?>(), server.sets)
        assertEquals(sendToQuest, agent.state.value.vban)
        assertEquals(sendToQuest, store.vban)

        agent.editVban(sendToQuest.copy(enabled = false))
        agent.refresh()
        assertNull(server.target)
    }

    @Test
    fun unconfiguredAgentLeavesAServerWithoutVbanAlone() {
        val agent = agent()
        agent.refresh()
        assertEquals(emptyList<StreamProtocol.VbanTarget?>(), server.sets)
        assertNull(store.vban)
    }

    @Test
    fun aRestartedServerGetsTheTargetAgain() {
        store.vban = sendToQuest
        val agent = agent()
        agent.refresh()
        server.running = false
        server.target = null
        agent.refresh()
        assertEquals(ServerState.NotRunning, agent.state.value.server)

        server.running = true
        agent.refresh()
        assertEquals(quest, server.target)
    }

    @Test
    fun startLaunchesTheServerAndWaitsForIt() = runBlocking {
        server.running = false
        val agent = agent()
        agent.startServer()
        assertEquals(1, adb.launches)
        assertNull("no message once it is running", agent.state.value.operation)
        agent.refresh()
        assertEquals(ServerState.Running(StreamProtocol.Status(96, emptyList(), null)), agent.state.value.server)
    }

    @Test
    fun startReportsWhyTheLaunchFailed() = runBlocking {
        server.running = false
        adb.failure = "not set up"
        val agent = agent()
        agent.startServer()
        assertEquals(Operation.Failed(Task.Start, "not set up"), agent.state.value.operation)
    }

    @Test
    fun startReportsAServerThatNeverAnswers() = runBlocking {
        server.running = false
        adb.startsServer = false
        val agent = agent()
        agent.startServer()
        val operation = agent.state.value.operation
        assertTrue(operation is Operation.Failed && SERVER_LOG_PATH in operation.message)
    }

    @Test
    fun stopStopsTheServer() = runBlocking {
        val agent = agent()
        agent.stopServer()
        assertFalse(server.running)
        assertNull(agent.state.value.operation)
    }

    @Test
    fun approveShowsTheHeadsetsAnswer() = runBlocking {
        val agent = agent()
        agent.approve()
        assertEquals(listOf("choose Always allow"), adb.progress)
        assertEquals(Operation.Done(Task.Approve, "allowed"), agent.state.value.operation)
    }
}
