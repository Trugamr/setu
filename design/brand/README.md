# Setu brand

<img src="setu-icon-dark.svg" width="128" alt="The Setu icon, dark: cream dots on a near-black tile, with an arch cut from the bottom and one red dot top right."> <img src="setu-icon-light.svg" width="128" alt="The Setu icon, light: near-black dots on a cream tile.">

Setu means "bridge". The mark is a speaker grille with an arch cut out of it: sound and a bridge
in one shape. The top corners are dropped, so the grille also reads as a headset seen from the
front, with the arch as the nose gap. One red dot is the power light.

## Colours

| Name   | Hex       | Use                                                        |
| ------ | --------- | ---------------------------------------------------------- |
| Ink    | `#15141A` | Backgrounds, the icon tile, text on Signal                 |
| Cream  | `#F5ECDC` | The grille, text on Ink                                    |
| Signal | `#FF3B2F` | The one accent: the power light, primary actions, live state |

Signal is used sparingly, once per view where possible, the way the icon uses one red dot among
fifty-odd cream ones. Put Ink text on Signal, not Cream: cream on red falls short of 4.5:1
contrast. The agent's UI steps up from Ink for raised surfaces and lines (`Theme.kt`).

## The mark

- A 7 × 5 grille on a 9-unit grid, centred in a 100-unit tile, dots of radius 2.9. It spans
  60 × 42 units, which leaves about 20 units of clear space on every side.
- An arch of radius 23, centred at (50, 78), removes the dots at the bottom centre.
- The two top-corner dots are dropped.
- The top row's rightmost dot, at (68, 32), is Signal.

It comes in two themes, and Signal is the same in both:

- **Dark**, the default: Cream dots on an Ink tile. The app icons use it.
- **Light**: Ink dots on a Cream tile, for light backgrounds such as the README in light mode.

Keep the dots round and flat: no gradients, shadows or outlines. Use only these two colourings
of the grille, and don't move the red dot.

At 16 px the full mark blurs, so `setu-icon-16-{theme}.svg` redraws it on the pixel grid: two-pixel dots
on a three-pixel pitch, same silhouette, same red dot. Each theme has one.

## Files

Each file comes as `-dark` and `-light`.

| File                          | What                                                           |
| ----------------------------- | -------------------------------------------------------------- |
| `setu-icon-{theme}.svg`       | The app icon: mark on its tile                                 |
| `setu-mark-{theme}.svg`       | The mark alone, transparent: dark for Ink backgrounds, light for Cream |
| `setu-icon-16-{theme}.svg`    | The 16 px cut                                                  |
| `png/{theme}/setu-icon-*.png` | Exports from 16 to 1024 px (16 px from the 16 px cut)          |
| `setu-{theme}.ico`            | Windows icon, 16 to 256 px. The desktop app embeds the dark one |

The agent's launcher icon (`apps/agent/src/main/res`) is an adaptive icon built from the same
mark, with a monochrome layer for themed icons.

Everything here comes from `generate.py`, which also writes the launcher icon. To change the mark,
change it there and run both scripts. `render.py` needs Chromium.

```sh
python3 design/brand/generate.py
python3 design/brand/render.py
```

The explorations that led here are in `design/icons/` (round 9 is the last).
