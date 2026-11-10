"""Setu's colour tokens and the contrast each pairing must keep.

The single source for the palette: generate.py writes it out for the desktop app
(apps/desktop/src/palette.rs), the agent (Palette.kt, colors.xml), the browser installer
(apps/web/src/palette.css) and the brand README, and
refuses to write anything while a pairing below falls short of its minimum.
"""

# (name, hex, use). Names are CamelCase; each platform derives its own spelling.
TOKENS = [
    # The brand.
    ("Ink", "#15141A", "Page background, the icon tile, text on Cream"),
    ("Cream", "#F5ECDC", "Text, the grille, focus, the status card while sound flows, on switches and on SignalFill"),
    ("Signal", "#FF3B2F", "The one accent: the power light, the action that starts things, errors"),
    # Steps up from Ink for the UI.
    ("InkRaised", "#1F1E25", "Cards, groups, dialogs, popovers and menus"),
    ("InkHigh", "#2A2930", "Dividers inside groups, disabled buttons, off switch tracks"),
    ("InkHover", "#36353D", "Hover and pressed on raised surfaces, the selected menu item"),
    ("Unlit", "#403F47", "Grille dots that are off"),
    ("Line", "#4A4851", "Popover outlines, and secondary text on Cream. Never a control's outline"),
    ("Edge", "#75727D", "Outlines of controls: inputs, selects, outline buttons, off switches and their thumbs"),
    ("CreamMuted", "#A9A398", "Secondary text and placeholders"),
    ("SignalFill", "#C4261C", "Filled Signal buttons, deep enough for Cream text"),
    ("SignalFillHover", "#CC2A1F", "SignalFill under the pointer"),
    ("SignalFillPressed", "#B32218", "SignalFill while pressed"),
]

HEX = {name: hex_ for name, hex_, _ in TOKENS}

# Minimums: WCAG 2.2 asks 4.5:1 for text (1.4.3) and 3:1 for the parts of a control that show
# what it is and its state (1.4.11). "Quiet" marks must be seen without competing: 1.4:1.
TEXT, CONTROL, QUIET = 4.5, 3.0, 1.4

# (foreground, background, minimum, where). A background may be "Cream 25% over Ink": the
# text selection, which is translucent.
PAIRS = [
    ("Cream", "Ink", TEXT, "Body text"),
    ("Cream", "InkRaised", TEXT, "Text on cards, dialogs and menus"),
    ("Cream", "InkHover", TEXT, "Text on a hovered or selected menu item"),
    ("CreamMuted", "Ink", TEXT, "Secondary text, placeholders"),
    ("CreamMuted", "InkRaised", TEXT, "Secondary text on cards"),
    ("CreamMuted", "InkHigh", TEXT, "A disabled button's label"),
    ("CreamMuted", "InkHover", TEXT, "Secondary text on a hovered item"),
    ("Ink", "Cream", TEXT, "Text on the status card while sound flows"),
    ("Line", "Cream", TEXT, "Secondary text on that card"),
    ("Cream", "Ink", TEXT, "The Stop button on that card"),
    ("Signal", "Ink", TEXT, "Error text"),
    ("Signal", "InkRaised", TEXT, "Error text on a card or in a dialog"),
    ("Cream", "SignalFill", TEXT, "Primary button label"),
    ("Cream", "SignalFillHover", TEXT, "Primary button label, hovered"),
    ("Cream", "SignalFillPressed", TEXT, "Primary button label, pressed"),
    ("Cream", "Cream 25% over Ink", TEXT, "Selected text"),
    ("Cream", "Cream 25% over InkRaised", TEXT, "Selected text on a card or in a dialog"),
    ("Edge", "Ink", CONTROL, "Input, select and outline button outlines"),
    ("Edge", "InkRaised", CONTROL, "Control outlines on a card, the off switch"),
    ("Edge", "InkHigh", CONTROL, "The off switch thumb on its track"),
    ("Cream", "InkRaised", CONTROL, "The on switch track, the outline of the status card while waiting, focus on a card"),
    ("Ink", "Cream", CONTROL, "The on switch thumb on its track"),
    ("Cream", "Ink", CONTROL, "The focus ring and the text cursor"),
    ("Signal", "Ink", CONTROL, "The power light"),
    ("Unlit", "InkRaised", QUIET, "Grille dots that are off"),
    ("Ink 20% over Cream", "Cream", QUIET, "Level meter dots that are off, on the status card"),
]


def rgb(hex_):
    h = hex_.lstrip("#")
    return tuple(int(h[i : i + 2], 16) for i in (0, 2, 4))


def resolve(spec):
    """A token's RGB, or for "A N% over B" the blend of A at N% opacity over B."""
    if " over " in spec:
        top, bottom = spec.split(" over ")
        name, percent = top.split()
        alpha = float(percent.rstrip("%")) / 100
        a, b = rgb(HEX[name]), rgb(HEX[bottom])
        return tuple(round(alpha * x + (1 - alpha) * y) for x, y in zip(a, b))
    return rgb(HEX[spec])


def luminance(color):
    def channel(c):
        c /= 255
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = (channel(c) for c in color)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(fg, bg):
    hi, lo = sorted((luminance(resolve(fg)), luminance(resolve(bg))), reverse=True)
    return (hi + 0.05) / (lo + 0.05)


def failures():
    return [
        f"{fg} on {bg}: {contrast(fg, bg):.2f}:1, needs {minimum}:1 ({where})"
        for fg, bg, minimum, where in PAIRS
        if contrast(fg, bg) < minimum
    ]


def snake(name):
    return "".join("_" + c.lower() if c.isupper() else c for c in name).lstrip("_")
