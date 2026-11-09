package dev.trugamr.setu

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import dev.trugamr.setu.protocol.StreamProtocol
import java.net.InetSocketAddress

@Composable
fun AgentScreen(
    state: AgentState,
    canWriteSecureSettings: Boolean,
    onStart: () -> Unit,
    onStop: () -> Unit,
    onVbanChange: (VbanSettings) -> Unit,
) {
    Column(
        modifier = Modifier
            .verticalScroll(rememberScrollState())
            .padding(32.dp)
            .widthIn(max = 640.dp),
        verticalArrangement = Arrangement.spacedBy(32.dp),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Setu", style = MaterialTheme.typography.headlineMedium)
            if (!canWriteSecureSettings) {
                Text(
                    "Not set up. Connect the headset to a PC and run Setu's setup once.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        ServerSection(state.server, state.operation, canStart = canWriteSecureSettings, onStart, onStop)
        VbanSection(state.vban, onVbanChange)
        ReceiverSetupSection(state.vban, state.headsetAddress)
    }
}

@Composable
private fun Section(title: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(title, style = MaterialTheme.typography.titleLarge)
        content()
    }
}

@Composable
private fun ServerSection(
    server: ServerState,
    operation: Operation?,
    canStart: Boolean,
    onStart: () -> Unit,
    onStop: () -> Unit,
) {
    val idle = operation !is Operation.Working
    Section("Capture server") {
        when (server) {
            ServerState.Checking -> Text("Checking…")
            ServerState.NotRunning -> {
                Text("Not running")
                Button(onClick = onStart, enabled = idle && canStart) { Text("Start") }
            }
            is ServerState.Running -> {
                val status = server.status
                Text("Running, capturing in ${status.blockFrames}-frame blocks")
                Text(
                    status.receivers.firstOrNull()?.let { "Setu receiver: ${it.name} (${it.address})" }
                        ?: "Setu receiver: none connected",
                )
                Text(
                    status.vban?.let { "VBAN: sending “${it.streamName}” to ${it.hostAndPort()}" }
                        ?: "VBAN: off",
                )
                OutlinedButton(onClick = onStop, enabled = idle) { Text("Stop") }
            }
        }
        OperationStatus(operation)
    }
}

@Composable
private fun OperationStatus(operation: Operation?) {
    when (operation) {
        null -> {}
        is Operation.Working -> Row(
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
            Text(operation.message)
        }
        is Operation.Done -> Text(operation.message, color = MaterialTheme.colorScheme.onSurfaceVariant)
        is Operation.Failed -> Text(operation.message, color = MaterialTheme.colorScheme.error)
    }
}

@Composable
private fun VbanSection(settings: VbanSettings, onChange: (VbanSettings) -> Unit) {
    Section("VBAN") {
        Text(
            "Sends the audio to any VBAN receiver, such as Voicemeeter, without the Setu desktop app.",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text("Send to a VBAN receiver")
            Switch(checked = settings.enabled, onCheckedChange = { onChange(settings.copy(enabled = it)) })
        }
        OutlinedTextField(
            value = settings.host,
            onValueChange = { onChange(settings.copy(host = it)) },
            label = { Text("PC's IP address") },
            placeholder = { Text("192.168.1.20") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
            modifier = Modifier.fillMaxWidth(),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedTextField(
                value = settings.port,
                onValueChange = { onChange(settings.copy(port = it)) },
                label = { Text("Port") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                modifier = Modifier.weight(1f),
            )
            OutlinedTextField(
                value = settings.streamName,
                onValueChange = { onChange(settings.copy(streamName = it)) },
                label = { Text("Stream name") },
                singleLine = true,
                modifier = Modifier.weight(2f),
            )
        }
        val validated = settings.validate()
        if (validated is VbanSettings.Validated.Invalid) {
            Text(validated.message, color = MaterialTheme.colorScheme.error)
        }
    }
}

@Composable
private fun ReceiverSetupSection(settings: VbanSettings, headsetAddress: String?) {
    Section("Set up your VBAN receiver") {
        Text(
            "Add an incoming stream on the PC with these details. In Voicemeeter: Menu, VBAN, then an " +
                "incoming stream.",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Detail("Stream name", settings.streamName.trim().ifEmpty { "—" })
        Detail("IP address from", headsetAddress ?: "Not connected to Wi-Fi")
        Detail("Port", settings.port.trim().ifEmpty { "—" })
        Text(
            "The receiver's network buffer sets the delay: in Voicemeeter, Net Quality Optimal is the " +
                "shortest; choose Fast if the sound crackles. Allow the receiver through the PC's " +
                "firewall for UDP on this port.",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun Detail(label: String, value: String) {
    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        Text(label, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value, style = MaterialTheme.typography.bodyLarge)
    }
}


@Preview(widthDp = 720, heightDp = 1200)
@Composable
private fun AgentScreenPreview() {
    val vban = VbanSettings(enabled = true, host = "192.168.1.20")
    val status = StreamProtocol.Status(
        96,
        listOf(StreamProtocol.Receiver("studio-pc", "192.168.1.20:50000")),
        StreamProtocol.VbanTarget(InetSocketAddress("192.168.1.20", 6980), "setu"),
    )
    MaterialTheme {
        AgentScreen(
            state = AgentState(ServerState.Running(status), vban, "192.168.1.42"),
            canWriteSecureSettings = true,
            onStart = {},
            onStop = {},
            onVbanChange = {},
        )
    }
}
