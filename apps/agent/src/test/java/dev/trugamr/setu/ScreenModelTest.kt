package dev.trugamr.setu

import dev.trugamr.setu.protocol.StreamProtocol
import java.net.InetSocketAddress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ScreenModelTest {
    private val vban = StreamProtocol.VbanTarget(InetSocketAddress("192.168.1.20", 6980), "setu")
    private val desktop = StreamProtocol.Receiver("studio-pc", "192.168.1.20:50000")

    private fun running(receivers: List<StreamProtocol.Receiver> = emptyList(), vban: StreamProtocol.VbanTarget? = null) =
        ServerState.Running(StreamProtocol.Status(96, receivers, vban))

    @Test
    fun withoutSetupThereIsNothingToStart() {
        val card = statusCard(AgentState(server = ServerState.NotRunning), canSetUp = false)
        assertEquals("Set up once", card.title)
        assertNull(card.action)
    }

    @Test
    fun offOffersStart() {
        val card = statusCard(AgentState(server = ServerState.NotRunning), canSetUp = true)
        assertEquals(Look.Plain, card.look)
        assertEquals(CardAction("Start", Task.Start), card.action)
    }

    @Test
    fun onWithNobodyListeningShowsTheAddressToConnectTo() {
        val card = statusCard(AgentState(server = running(), headsetAddress = "192.168.1.42"), canSetUp = true)
        assertEquals(Look.Waiting, card.look)
        assertEquals("192.168.1.42", card.address)
        assertEquals(Task.Stop, card.action?.task)
    }

    @Test
    fun onWithoutWifiSaysSo() {
        val card = statusCard(AgentState(server = running()), canSetUp = true)
        assertNull(card.address)
        assertTrue(card.sentence, "Wi-Fi" in card.sentence)
    }

    @Test
    fun sendingNamesWhereTheSoundGoes() {
        val state = AgentState(server = running(listOf(desktop), vban), headsetAddress = "192.168.1.42")
        val card = statusCard(state, canSetUp = true)
        assertEquals(Look.Flowing, card.look)
        assertEquals("To studio-pc and over VBAN.", card.sentence)
        assertEquals(
            listOf(Fact("2 ms", "blocks"), Fact("2", "receivers"), Fact("192.168.1.42", "this headset")),
            card.facts,
        )
    }

    @Test
    fun destinationsReadAsASentence() {
        assertEquals("Over VBAN.", destinations(emptyList(), vban = true))
        assertEquals("To a.", destinations(listOf("a"), vban = false))
        assertEquals("To a, b and c and over VBAN.", destinations(listOf("a", "b", "c"), vban = true))
    }

    @Test
    fun blocksInMilliseconds() {
        assertEquals("2 ms", blockMs(96))
        assertEquals("2.5 ms", blockMs(120))
    }

    @Test
    fun anOperationInProgressDisablesTheButton() {
        val state = AgentState(server = ServerState.NotRunning, operation = Operation.Working(Task.Start, "Waiting for the capture server…"))
        val card = statusCard(state, canSetUp = true)
        assertEquals("Starting", card.title)
        assertEquals("Waiting for the capture server…", card.sentence)
        assertTrue(card.busy)
        assertEquals(false, card.action?.enabled)
    }

    @Test
    fun aFailedStartShowsWhyUntilTheServerRuns() {
        val failed = Operation.Failed(Task.Start, "The capture server didn't start.")
        val card = statusCard(AgentState(server = ServerState.NotRunning, operation = failed), canSetUp = true)
        assertEquals("Couldn't start", card.title)
        assertTrue(card.problem)
        assertEquals(CardAction("Try again", Task.Start), card.action)

        val later = statusCard(AgentState(server = running(), headsetAddress = "192.168.1.42", operation = failed), canSetUp = true)
        assertEquals("Ready", later.title)
    }

    @Test
    fun outputRowsSayWhoIsConnected() {
        assertEquals(OutputRow("studio-pc", "Connected"), desktopRow(running(listOf(desktop))))
        assertEquals(OutputRow("Nobody connected yet", "Waiting"), desktopRow(running()))
        assertEquals(OutputRow("192.168.1.20:6980"), vbanRow(VbanSettings.of(vban)))
        assertEquals(OutputRow("Not set up yet"), vbanRow(VbanSettings(enabled = true)))
    }
}
