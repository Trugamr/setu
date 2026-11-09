package dev.trugamr.setu

import dev.trugamr.setu.server.PlaybackCapture
import java.util.Locale

// What the agent's screen shows, worked out from AgentState without Compose, so the wording and
// the states can be tested on the JVM. AgentScreen only lays these out.

/** How the status card looks. */
enum class Look {
    /** Ink fill: off, busy, not set up, or a problem. */
    Plain,

    /** Ink fill and a Cream outline: on, but nothing is listening yet. */
    Waiting,

    /** Cream fill: sound is going somewhere. */
    Flowing,
}

/** The status card's button. */
data class CardAction(val label: String, val task: Task, val enabled: Boolean = true)

/** A figure on the card while sending, such as "2 ms" over "blocks". */
data class Fact(val value: String, val label: String)

data class StatusCard(
    val look: Look,
    val title: String,
    val sentence: String,
    /** Whether [sentence] says what went wrong. */
    val problem: Boolean = false,
    /** A label line under the sentence, over [address] when there is one. */
    val label: String? = null,
    /** An address to type on the PC, shown large. */
    val address: String? = null,
    val facts: List<Fact> = emptyList(),
    /** Whether something is in progress: the grille and a bar animate. */
    val busy: Boolean = false,
    val action: CardAction? = null,
)

/** A row in the Outputs group. */
data class OutputRow(val subtitle: String, val trailing: String? = null)

fun statusCard(state: AgentState, canSetUp: Boolean): StatusCard {
    val server = state.server
    val running = server is ServerState.Running
    when (val operation = state.operation) {
        is Operation.Working -> return StatusCard(
            look = Look.Plain,
            title = when (operation.task) {
                Task.Start -> "Starting"
                Task.Stop -> "Stopping"
                Task.Approve -> "Setting up"
            },
            sentence = operation.message,
            busy = true,
            action = if (operation.task == Task.Stop) CardAction("Stop", Task.Stop, enabled = false)
            else CardAction("Start", Task.Start, enabled = false),
        )
        is Operation.Failed -> {
            // A failure shows only while it describes the server: a start that failed
            // while nothing runs, a stop that failed while the server runs.
            val current = when (operation.task) {
                Task.Start, Task.Approve -> !running
                Task.Stop -> running
            }
            if (current) return StatusCard(
                look = Look.Plain,
                title = when (operation.task) {
                    Task.Start -> "Couldn't start"
                    Task.Stop -> "Couldn't stop"
                    Task.Approve -> "Couldn't finish setup"
                },
                sentence = operation.message,
                problem = true,
                action = when (operation.task) {
                    Task.Start -> CardAction("Try again", Task.Start, enabled = canSetUp)
                    Task.Stop -> CardAction("Try again", Task.Stop)
                    // Approval is asked for from the PC, during setup.
                    Task.Approve -> null
                },
            )
        }
        is Operation.Done, null -> {}
    }
    return when (server) {
        ServerState.Checking -> StatusCard(
            look = Look.Plain,
            title = "Checking",
            sentence = "Looking for the capture server…",
            busy = true,
        )
        ServerState.NotRunning -> if (!canSetUp) StatusCard(
            look = Look.Plain,
            title = "Set up once",
            sentence = "Connect the headset to your PC by USB and run Setu's setup. You only do this once.",
        ) else StatusCard(
            look = Look.Plain,
            title = "Off",
            sentence = (state.operation as? Operation.Done)?.message
                ?: "Start to send this headset's sound to your PC.",
            action = CardAction("Start", Task.Start),
        )
        is ServerState.Running -> running(server, state.headsetAddress)
    }
}

private fun running(server: ServerState.Running, headsetAddress: String?): StatusCard {
    val status = server.status
    val stop = CardAction("Stop", Task.Stop)
    if (status.receivers.isEmpty() && status.vban == null) {
        return if (headsetAddress == null) StatusCard(
            look = Look.Waiting,
            title = "Ready",
            sentence = "Connect the headset to Wi-Fi so your PC can reach it.",
            action = stop,
        ) else StatusCard(
            look = Look.Waiting,
            title = "Ready",
            sentence = "Nobody is listening yet.",
            label = "In Setu on your PC, connect to",
            address = headsetAddress,
            action = stop,
        )
    }
    val facts = buildList {
        add(Fact(blockMs(status.blockFrames), "blocks"))
        add(Fact((status.receivers.size + if (status.vban != null) 1 else 0).toString(), "receivers"))
        headsetAddress?.let { add(Fact(it, "this headset")) }
    }
    return StatusCard(
        look = Look.Flowing,
        title = "Sending",
        sentence = destinations(status.receivers.map { it.name }, status.vban != null),
        facts = facts,
        action = stop,
    )
}

/** Where the sound goes, as a sentence: "To studio-pc and over VBAN." */
fun destinations(receivers: List<String>, vban: Boolean): String {
    val names = when (receivers.size) {
        0 -> null
        1 -> receivers[0]
        else -> receivers.dropLast(1).joinToString(", ") + " and " + receivers.last()
    }
    return when {
        names != null && vban -> "To $names and over VBAN."
        names != null -> "To $names."
        else -> "Over VBAN."
    }
}

/** A block of [frames] at the capture rate, in milliseconds: "2 ms", "2.5 ms". */
fun blockMs(frames: Int): String {
    val ms = frames * 1000.0 / PlaybackCapture.SAMPLE_RATE
    val text = if (ms == Math.rint(ms)) ms.toLong().toString() else String.format(Locale.ROOT, "%.1f", ms)
    return "$text ms"
}

fun desktopRow(server: ServerState): OutputRow = when (server) {
    is ServerState.Running -> {
        val names = server.status.receivers.map { it.name }
        if (names.isEmpty()) OutputRow("Nobody connected yet", "Waiting")
        else OutputRow(names.joinToString(", "), "Connected")
    }
    else -> OutputRow("Connects from Setu on your PC")
}

fun vbanRow(settings: VbanSettings): OutputRow = when (val validated = settings.validate()) {
    VbanSettings.Validated.Off -> OutputRow("For apps like Voicemeeter")
    is VbanSettings.Validated.Send -> OutputRow(validated.target.hostAndPort())
    is VbanSettings.Validated.Invalid -> OutputRow("Not set up yet")
}
