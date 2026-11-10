package dev.trugamr.setu

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import dev.trugamr.setu.protocol.StreamProtocol
import java.net.InetSocketAddress

/**
 * The agent's one screen: the status card, then the outputs and VBAN's settings. One column
 * below [Dimens.TwoColumns] wide, such as a phone held upright; two above it, such as the Quest's
 * panel. What each part says comes from ScreenModel.
 */
@Composable
fun AgentScreen(
    state: AgentState,
    canSetUp: Boolean,
    onStart: () -> Unit,
    onStop: () -> Unit,
    onVbanChange: (VbanSettings) -> Unit,
) {
    val card = statusCard(state, canSetUp)
    val onAction = { task: Task -> if (task == Task.Stop) onStop() else onStart() }
    var editing by rememberSaveable { mutableStateOf<VbanField?>(null) }
    var guide by rememberSaveable { mutableStateOf(false) }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        if (maxWidth < Dimens.TwoColumns) {
            Column(
                Modifier
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = Dimens.Margin)
                    .padding(bottom = Dimens.Margin),
            ) {
                AppTitle()
                StatusCardView(card, onAction)
                Outputs(state, onVbanChange)
                VbanGroup(state.vban, onEdit = { editing = it }, onGuide = { guide = true })
            }
        } else {
            Row(
                Modifier.padding(horizontal = Dimens.Margin).padding(bottom = Dimens.Margin),
                horizontalArrangement = Arrangement.spacedBy(Dimens.SectionGap),
            ) {
                Column(Modifier.weight(0.45f).verticalScroll(rememberScrollState())) {
                    AppTitle()
                    StatusCardView(card, onAction)
                }
                Column(Modifier.weight(0.55f).verticalScroll(rememberScrollState())) {
                    // Lines the first group's top up with the card's.
                    Spacer(Modifier.height(TITLE_HEIGHT - SECTION_LABEL_HEIGHT))
                    Outputs(state, onVbanChange)
                    VbanGroup(state.vban, onEdit = { editing = it }, onGuide = { guide = true })
                }
            }
        }
    }

    editing?.let { field ->
        EditDialog(
            field = field,
            initial = field.get(state.vban),
            onSave = {
                onVbanChange(field.set(state.vban, it))
                editing = null
            },
            onDismiss = { editing = null },
        )
    }
    if (guide) GuideDialog(state.vban, state.headsetAddress, onDismiss = { guide = false })
}

/** The app title's height with its padding: 8 + 40 + 16. */
private val TITLE_HEIGHT = 64.dp

/** A section label's height with its padding: 24 + 18 + 8. */
private val SECTION_LABEL_HEIGHT = 50.dp

@Composable
private fun AppTitle() {
    Text(
        "Setu",
        style = Type.AppTitle,
        color = Palette.Cream,
        modifier = Modifier.padding(start = 4.dp, top = 8.dp, bottom = Dimens.Gap),
    )
}

@Composable
private fun StatusCardView(card: StatusCard, onAction: (Task) -> Unit) {
    val flowing = card.look == Look.Flowing
    val content = if (flowing) Palette.Ink else Palette.Cream
    val secondary = if (flowing) Palette.Line else Palette.CreamMuted
    Surface(
        shape = Shapes.Card,
        color = if (flowing) Palette.Cream else Palette.InkRaised,
        contentColor = content,
        border = if (card.look == Look.Waiting) BorderStroke(2.dp, Palette.Cream) else null,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(Dimens.CardPadding)) {
            Row(horizontalArrangement = Arrangement.spacedBy(Dimens.Gap)) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Dimens.Tight)) {
                    Text(card.title, style = Type.Title)
                    Text(
                        card.sentence,
                        style = Type.Sentence,
                        color = when {
                            card.problem -> Palette.Signal
                            card.look == Look.Waiting -> Palette.Cream
                            else -> secondary
                        },
                    )
                }
                Grille(card.look, card.busy)
            }
            if (card.label != null || card.address != null) {
                Column(Modifier.padding(top = Dimens.Gap)) {
                    card.label?.let { Text(it, style = Type.Label, color = secondary) }
                    card.address?.let { Text(it, style = Type.Address) }
                }
            }
            if (card.facts.isNotEmpty()) {
                Row(Modifier.padding(top = Dimens.Gap), horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                    for (fact in card.facts) {
                        Column {
                            Text(fact.value, style = Type.Fact)
                            Text(fact.label, style = Type.FactLabel, color = secondary)
                        }
                    }
                }
            }
            if (card.busy) {
                LinearProgressIndicator(
                    color = Palette.Cream,
                    trackColor = Palette.InkHigh,
                    strokeCap = StrokeCap.Round,
                    gapSize = 0.dp,
                    modifier = Modifier.padding(top = Dimens.Gap).fillMaxWidth().height(4.dp),
                )
            }
            card.action?.let { action ->
                Spacer(Modifier.height(Dimens.CardPadding))
                CardButton(action, card.look) { onAction(action.task) }
            }
        }
    }
}

@Composable
private fun CardButton(action: CardAction, look: Look, onClick: () -> Unit) {
    val modifier = Modifier.fillMaxWidth().height(Dimens.ControlHeight)
    val label = @Composable { Text(action.label, style = Type.Button) }
    when (look) {
        Look.Waiting -> OutlinedButton(
            onClick = onClick,
            enabled = action.enabled,
            shape = Shapes.Control,
            border = BorderStroke(1.5.dp, Palette.Edge),
            colors = ButtonDefaults.outlinedButtonColors(contentColor = Palette.Cream),
            modifier = modifier,
            content = { label() },
        )
        Look.Flowing -> Button(
            onClick = onClick,
            enabled = action.enabled,
            shape = Shapes.Control,
            colors = ButtonDefaults.buttonColors(containerColor = Palette.Ink, contentColor = Palette.Cream),
            modifier = modifier,
            content = { label() },
        )
        Look.Plain -> Button(
            onClick = onClick,
            enabled = action.enabled,
            shape = Shapes.Control,
            colors = ButtonDefaults.buttonColors(
                containerColor = Palette.SignalFill,
                contentColor = Palette.Cream,
                disabledContainerColor = Palette.InkHigh,
                disabledContentColor = Palette.CreamMuted,
            ),
            modifier = modifier,
            content = { label() },
        )
    }
}

/**
 * The grille from the icon, five dots by three, as tall as the title's line. Lit when the server
 * is on; while busy, lit dots sweep through it column by column.
 */
@Composable
private fun Grille(look: Look, busy: Boolean) {
    val sweep = if (busy) {
        val transition = rememberInfiniteTransition(label = "grille")
        transition.animateFloat(0f, DOTS + 1f, infiniteRepeatable(tween(1600, easing = LinearEasing)), label = "sweep").value
    } else {
        0f
    }
    val (lit, unlit) = when (look) {
        Look.Flowing -> Palette.Ink to Palette.Ink
        Look.Waiting -> Palette.Cream to Palette.Cream
        Look.Plain -> Palette.Cream to Palette.Unlit
    }
    Canvas(Modifier.size(width = 60.dp, height = 34.dp)) {
        val dot = 8.dp.toPx()
        val step = 13.dp.toPx()
        for (col in 0 until COLUMNS) {
            for (row in 0 until ROWS) {
                // Column by column, bottom to top, the order a level meter fills.
                val index = col * ROWS + (ROWS - 1 - row)
                val on = look != Look.Plain || index < sweep.toInt()
                drawCircle(
                    color = if (on) lit else unlit,
                    radius = dot / 2,
                    center = Offset(col * step + dot / 2, row * step + dot / 2),
                )
            }
        }
    }
}

private const val COLUMNS = 5
private const val ROWS = 3
private const val DOTS = COLUMNS * ROWS

@Composable
private fun SectionLabel(text: String) {
    Text(
        text,
        style = Type.Label,
        color = Palette.CreamMuted,
        modifier = Modifier.padding(start = Dimens.CardPadding, top = Dimens.SectionGap, bottom = 8.dp),
    )
}

@Composable
private fun Group(content: @Composable ColumnScope.() -> Unit) {
    Surface(shape = Shapes.Group, color = Palette.InkRaised, contentColor = Palette.Cream, modifier = Modifier.fillMaxWidth()) {
        Column(content = content)
    }
}

@Composable
private fun Divider() = HorizontalDivider(thickness = 1.dp, color = Palette.InkHigh)

@Composable
private fun Outputs(state: AgentState, onVbanChange: (VbanSettings) -> Unit) {
    SectionLabel("Outputs")
    Group {
        val desktop = desktopRow(state.server)
        OutputRowView(title = "Setu desktop", row = desktop, icon = { drawMonitor(it) }) {
            desktop.trailing?.let {
                Text(it, style = Type.Label, color = if (it == "Connected") Palette.Cream else Palette.CreamMuted)
            }
        }
        Divider()
        val toggle = { onVbanChange(state.vban.copy(enabled = !state.vban.enabled)) }
        OutputRowView(title = "VBAN", row = vbanRow(state.vban), icon = { drawWave(it) }, onClick = toggle) {
            Switch(
                checked = state.vban.enabled,
                onCheckedChange = { toggle() },
                colors = SwitchDefaults.colors(
                    checkedTrackColor = Palette.Cream,
                    checkedThumbColor = Palette.Ink,
                    checkedBorderColor = Palette.Cream,
                    uncheckedTrackColor = Palette.InkHigh,
                    uncheckedThumbColor = Palette.Edge,
                    uncheckedBorderColor = Palette.Edge,
                ),
            )
        }
    }
}

@Composable
private fun OutputRowView(
    title: String,
    row: OutputRow,
    icon: DrawScope.(Color) -> Unit,
    onClick: (() -> Unit)? = null,
    trailing: @Composable () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = Dimens.RowHeight)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(start = 12.dp, end = Dimens.Gap, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Surface(shape = Shapes.Control, color = Palette.InkHigh, modifier = Modifier.size(Dimens.Tile)) {
            Box(contentAlignment = Alignment.Center) {
                Canvas(Modifier.size(20.dp)) { icon(Palette.Cream) }
            }
        }
        Column(Modifier.weight(1f)) {
            Text(title, style = Type.Row)
            Text(row.subtitle, style = Type.Label, color = Palette.CreamMuted)
        }
        trailing()
    }
}

/** A monitor, for the Setu desktop app, on a 24-unit grid scaled to the canvas. */
private fun DrawScope.drawMonitor(color: Color) {
    val u = size.width / 24
    val stroke = Stroke(width = 2 * u, cap = StrokeCap.Round)
    drawRoundRect(color, topLeft = Offset(3 * u, 4 * u), size = Size(18 * u, 12 * u), cornerRadius = CornerRadius(2 * u), style = stroke)
    drawLine(color, Offset(12 * u, 16 * u), Offset(12 * u, 20 * u), strokeWidth = 2 * u, cap = StrokeCap.Round)
    drawLine(color, Offset(9 * u, 20 * u), Offset(15 * u, 20 * u), strokeWidth = 2 * u, cap = StrokeCap.Round)
}

/** A waveform, for VBAN, on a 24-unit grid. */
private fun DrawScope.drawWave(color: Color) {
    val u = size.width / 24
    for ((x, half) in listOf(4 to 2, 8 to 5, 12 to 8, 16 to 4, 20 to 1)) {
        drawLine(color, Offset(x * u, (12 - half) * u), Offset(x * u, (12 + half) * u), strokeWidth = 2 * u, cap = StrokeCap.Round)
    }
}

/** A VBAN setting the user edits in a dialog. */
enum class VbanField(val title: String, val keyboard: KeyboardType) {
    Host("PC's IP address", KeyboardType.Uri),
    Port("Port", KeyboardType.Number),
    StreamName("Stream name", KeyboardType.Ascii);

    fun get(settings: VbanSettings) = when (this) {
        Host -> settings.host
        Port -> settings.port
        StreamName -> settings.streamName
    }

    fun set(settings: VbanSettings, value: String) = when (this) {
        Host -> settings.copy(host = value.trim())
        Port -> settings.copy(port = value.trim())
        StreamName -> settings.copy(streamName = value.trim())
    }

    fun problem(value: String) = when (this) {
        Host -> VbanSettings.hostProblem(value)
        Port -> VbanSettings.portProblem(value)
        StreamName -> VbanSettings.streamNameProblem(value)
    }
}

@Composable
private fun VbanGroup(settings: VbanSettings, onEdit: (VbanField) -> Unit, onGuide: () -> Unit) {
    if (!settings.enabled) return
    SectionLabel("VBAN")
    Group {
        for (field in VbanField.entries) {
            ValueRow(field.title, field.get(settings).ifBlank { "Not set" }) { onEdit(field) }
            Divider()
        }
        ValueRow("Set up Voicemeeter", null, onGuide)
    }
    (settings.validate() as? VbanSettings.Validated.Invalid)?.let {
        Text(
            it.message,
            style = Type.Label,
            color = Palette.Signal,
            modifier = Modifier.padding(start = Dimens.CardPadding, end = Dimens.CardPadding, top = 8.dp),
        )
    }
}

@Composable
private fun ValueRow(title: String, value: String?, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = Dimens.ValueRowHeight)
            .clickable(onClick = onClick)
            .padding(horizontal = Dimens.Gap),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(title, style = Type.Row, modifier = Modifier.weight(1f))
        value?.let { Text(it, style = Type.Sentence, color = Palette.CreamMuted) }
        Canvas(Modifier.size(width = 8.dp, height = 14.dp)) {
            val w = 1.75.dp.toPx()
            drawLine(Palette.CreamMuted, Offset(w, w), Offset(size.width - w, size.height / 2), strokeWidth = w, cap = StrokeCap.Round)
            drawLine(Palette.CreamMuted, Offset(size.width - w, size.height / 2), Offset(w, size.height - w), strokeWidth = w, cap = StrokeCap.Round)
        }
    }
}

/** A card-shaped dialog: 32 round with 20 padding, like the status card. */
@Composable
private fun SetuDialog(onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    Dialog(onDismissRequest = onDismiss) {
        Surface(shape = Shapes.Card, color = Palette.InkRaised, contentColor = Palette.Cream) {
            Column(Modifier.padding(Dimens.CardPadding), content = content)
        }
    }
}

@Composable
private fun DialogButtons(content: @Composable () -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(top = Dimens.CardPadding),
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
    ) { content() }
}

@Composable
private fun DialogButton(label: String, enabled: Boolean = true, onClick: () -> Unit) {
    TextButton(
        onClick = onClick,
        enabled = enabled,
        shape = Shapes.Control,
        colors = ButtonDefaults.textButtonColors(contentColor = Palette.Cream, disabledContentColor = Palette.CreamMuted),
        modifier = Modifier.height(Dimens.ControlHeight),
    ) { Text(label, style = Type.Button) }
}

/** Edits one VBAN setting. Saves only a value the setting can take, and only on Save. */
@Composable
private fun EditDialog(field: VbanField, initial: String, onSave: (String) -> Unit, onDismiss: () -> Unit) {
    var value by rememberSaveable { mutableStateOf(initial) }
    val problem = field.problem(value)
    // A field that starts empty isn't wrong yet; it is once it has been typed in.
    val showProblem = problem != null && (value != initial || initial.isNotBlank())
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    SetuDialog(onDismiss) {
        Text(field.title, style = Type.DialogTitle)
        Spacer(Modifier.height(Dimens.Gap))
        OutlinedTextField(
            value = value,
            onValueChange = { value = it },
            singleLine = true,
            isError = showProblem,
            supportingText = if (showProblem) ({ Text(problem!!, style = Type.Label) }) else null,
            shape = Shapes.Control,
            colors = fieldColors(),
            textStyle = Type.Row.copy(fontFeatureSettings = "tnum"),
            keyboardOptions = KeyboardOptions(keyboardType = field.keyboard, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { if (problem == null) onSave(value) }),
            modifier = Modifier.fillMaxWidth().focusRequester(focus),
        )
        DialogButtons {
            DialogButton("Cancel", onClick = onDismiss)
            DialogButton("Save", enabled = problem == null) { onSave(value) }
        }
    }
}

/**
 * Focus in Cream rather than Material's primary, Signal: the field is focused as the dialog opens,
 * and must not look like an error until it is one.
 */
@Composable
private fun fieldColors() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = Palette.Cream,
    focusedLabelColor = Palette.Cream,
    cursorColor = Palette.Cream,
    selectionColors = TextSelectionColors(handleColor = Palette.Cream, backgroundColor = Palette.Cream.copy(alpha = 0.25f)),
)

/** What to set up on the PC's side for VBAN, with this headset's values filled in. */
@Composable
private fun GuideDialog(settings: VbanSettings, headsetAddress: String?, onDismiss: () -> Unit) {
    SetuDialog(onDismiss) {
        Text("Set up Voicemeeter", style = Type.DialogTitle)
        Spacer(Modifier.height(Dimens.Tight))
        Text(
            "In Voicemeeter, open Menu, then VBAN, and add an incoming stream with these details.",
            style = Type.Sentence,
            color = Palette.CreamMuted,
        )
        Spacer(Modifier.height(Dimens.Gap))
        Surface(shape = Shapes.Control, color = Palette.Ink, contentColor = Palette.Cream) {
            Column(Modifier.padding(horizontal = Dimens.Gap, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Detail("Stream name", settings.streamName.trim().ifEmpty { "Not set" })
                Detail("IP address from", headsetAddress ?: "Not on Wi-Fi")
                Detail("Port", settings.port.trim().ifEmpty { "Not set" })
            }
        }
        Spacer(Modifier.height(Dimens.Gap))
        Text(
            "The receiver's network buffer sets the delay: Net Quality Optimal is the shortest; choose " +
                "Fast if the sound crackles. Allow Voicemeeter through the PC's firewall for UDP on this port.",
            style = Type.Label,
            color = Palette.CreamMuted,
        )
        DialogButtons { DialogButton("Done", onClick = onDismiss) }
    }
}

@Composable
private fun Detail(label: String, value: String) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        Text(label, style = Type.Sentence, color = Palette.CreamMuted)
        Spacer(Modifier.width(Dimens.Gap))
        Text(value, style = Type.Sentence.copy(fontFeatureSettings = "tnum"))
    }
}

internal fun previewState(server: ServerState, vban: VbanSettings = VbanSettings()) =
    AgentState(server = server, vban = vban, headsetAddress = "192.168.1.42")

internal val previewVban = VbanSettings(enabled = true, host = "192.168.1.20")
internal val previewSending = ServerState.Running(
    StreamProtocol.Status(
        96,
        listOf(StreamProtocol.Receiver("studio-pc", "192.168.1.20:50000")),
        StreamProtocol.VbanTarget(InetSocketAddress("192.168.1.20", 6980), "setu"),
    ),
)

@Composable
internal fun Preview(state: AgentState, canSetUp: Boolean = true) {
    SetuTheme {
        Surface(color = Palette.Ink) {
            AgentScreen(state, canSetUp, onStart = {}, onStop = {}, onVbanChange = {})
        }
    }
}

@Preview(name = "Sending", widthDp = 390, heightDp = 844)
@Composable
internal fun SendingPreview() = Preview(previewState(previewSending, previewVban))

@Preview(name = "Ready", widthDp = 390, heightDp = 844)
@Composable
internal fun ReadyPreview() = Preview(previewState(ServerState.Running(StreamProtocol.Status(96, emptyList(), null))))

@Preview(name = "Off", widthDp = 390, heightDp = 844)
@Composable
internal fun OffPreview() = Preview(previewState(ServerState.NotRunning))

@Preview(name = "Not set up", widthDp = 390, heightDp = 844)
@Composable
internal fun NotSetUpPreview() = Preview(previewState(ServerState.NotRunning), canSetUp = false)

@Preview(name = "Quest panel", widthDp = 1024, heightDp = 640)
@Composable
internal fun PanelPreview() = Preview(previewState(previewSending, previewVban))
