package dev.trugamr.setu

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

/** Setu's brand colours: cream on near-black ink, with signal red as the one accent. See design/brand. */
object Brand {
    val Ink = Color(0xFF15141A)
    val Cream = Color(0xFFF5ECDC)
    val Signal = Color(0xFFFF3B2F)

    // Steps up from Ink for raised surfaces, lines and secondary text.
    val InkRaised = Color(0xFF1F1E25)
    val InkHigh = Color(0xFF2A2930)
    val Line = Color(0xFF4A4851)
    val CreamMuted = Color(0xFFA9A398)
}

// Ink text on signal red: cream on red falls short of 4.5:1 contrast.
private val colors = darkColorScheme(
    primary = Brand.Signal,
    onPrimary = Brand.Ink,
    secondary = Brand.Cream,
    onSecondary = Brand.Ink,
    background = Brand.Ink,
    onBackground = Brand.Cream,
    surface = Brand.Ink,
    onSurface = Brand.Cream,
    surfaceVariant = Brand.InkHigh,
    onSurfaceVariant = Brand.CreamMuted,
    surfaceContainerLowest = Brand.Ink,
    surfaceContainerLow = Brand.InkRaised,
    surfaceContainer = Brand.InkRaised,
    surfaceContainerHigh = Brand.InkHigh,
    surfaceContainerHighest = Brand.InkHigh,
    outline = Brand.Line,
    outlineVariant = Brand.InkHigh,
)

@Composable
fun SetuTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = colors, content = content)
}
