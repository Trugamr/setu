package dev.trugamr.setu

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The agent's spacing, shapes and type. The radii nest inside the screen's own corners: about 48
 * round, less the 16 margin, is the card's 32; less the card's 20 padding, its button's 12. See
 * design/brand/README.md, "Shape and spacing"; the desktop app keeps the same rules in dimens.rs.
 */
object Dimens {
    /** From the screen's edge to the content. */
    val Margin = 16.dp
    val CardPadding = 20.dp

    /** Between lines that belong together, such as a title and its sentence. */
    val Tight = 4.dp

    /** Between groups of content. */
    val Gap = 16.dp

    /** Above a section's label. */
    val SectionGap = 24.dp
    val ControlHeight = 48.dp
    val Tile = 40.dp
    val RowHeight = 64.dp
    val ValueRowHeight = 52.dp

    /** Below this width the screen is one column; at or above it, two. */
    val TwoColumns = 600.dp
}

object Shapes {
    val Card = RoundedCornerShape(32.dp)
    val Group = RoundedCornerShape(24.dp)

    /** Buttons, fields, icon tiles, and boxes inside a card. */
    val Control = RoundedCornerShape(12.dp)
}

object Type {
    val AppTitle = TextStyle(fontSize = 32.sp, lineHeight = 40.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.5).sp)
    val Title = TextStyle(fontSize = 28.sp, lineHeight = 34.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.4).sp)
    val DialogTitle = TextStyle(fontSize = 22.sp, lineHeight = 28.sp, fontWeight = FontWeight.SemiBold)
    val Address = TextStyle(
        fontSize = 28.sp, lineHeight = 34.sp, fontWeight = FontWeight.SemiBold,
        letterSpacing = (-0.3).sp, fontFeatureSettings = "tnum",
    )
    val Fact = TextStyle(fontSize = 17.sp, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold, fontFeatureSettings = "tnum")
    val Row = TextStyle(fontSize = 16.sp, lineHeight = 22.sp)
    val Sentence = TextStyle(fontSize = 15.sp, lineHeight = 22.sp)
    val Button = TextStyle(fontSize = 16.sp, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold)
    val Label = TextStyle(fontSize = 13.sp, lineHeight = 18.sp)
    val FactLabel = TextStyle(fontSize = 12.sp, lineHeight = 16.sp)
}
