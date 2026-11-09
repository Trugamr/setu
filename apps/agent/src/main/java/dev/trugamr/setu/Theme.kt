package dev.trugamr.setu

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable

/**
 * Every Material role mapped to [Palette], so no default purple or pink shows through.
 * Text on Signal is Ink, since Cream on red is 3:1. Control outlines (text fields, outline
 * buttons, the off switch) are `outline`, in Edge, which keeps 3:1 on every surface; `outlineVariant`
 * is for dividers. Containers are Ink steps rather than tints, keeping Signal the one accent.
 * See design/brand/README.md.
 */
private val colors = darkColorScheme(
    primary = Palette.Signal,
    onPrimary = Palette.Ink,
    primaryContainer = Palette.InkHigh,
    onPrimaryContainer = Palette.Cream,
    inversePrimary = Palette.SignalPressed,
    secondary = Palette.Cream,
    onSecondary = Palette.Ink,
    secondaryContainer = Palette.InkHigh,
    onSecondaryContainer = Palette.Cream,
    tertiary = Palette.Cream,
    onTertiary = Palette.Ink,
    tertiaryContainer = Palette.InkHigh,
    onTertiaryContainer = Palette.Cream,
    background = Palette.Ink,
    onBackground = Palette.Cream,
    surface = Palette.Ink,
    onSurface = Palette.Cream,
    surfaceVariant = Palette.InkHigh,
    onSurfaceVariant = Palette.CreamMuted,
    // Tonal elevation lightens toward Cream instead of tinting surfaces red.
    surfaceTint = Palette.Cream,
    inverseSurface = Palette.Cream,
    inverseOnSurface = Palette.Ink,
    error = Palette.Signal,
    onError = Palette.Ink,
    errorContainer = Palette.InkRaised,
    onErrorContainer = Palette.Signal,
    outline = Palette.Edge,
    outlineVariant = Palette.Line,
    surfaceBright = Palette.InkHover,
    surfaceDim = Palette.Ink,
    surfaceContainerLowest = Palette.Ink,
    surfaceContainerLow = Palette.InkRaised,
    surfaceContainer = Palette.InkRaised,
    surfaceContainerHigh = Palette.InkHigh,
    surfaceContainerHighest = Palette.InkHigh,
)

@Composable
fun SetuTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = colors, content = content)
}
