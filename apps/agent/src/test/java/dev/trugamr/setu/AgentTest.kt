package dev.trugamr.setu

import dev.trugamr.setu.protocol.StreamProtocol
import java.net.InetSocketAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
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
    }

    private class MemoryStore(var vban: VbanSettings = VbanSettings()) : SettingsStore {
        override fun loadVban() = vban

        override fun saveVban(settings: VbanSettings) {
            vban = settings
        }
    }

    private val server = FakeServer()
    private val store = MemoryStore()

    private fun agent() = Agent(server, store) { "192.168.1.42" }

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
}
