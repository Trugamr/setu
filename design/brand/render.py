"""Renders Setu's PNG and ICO exports from the SVGs that generate.py writes.

Chromium draws each SVG onto a canvas at the exact pixel size and the page hands back the PNG
bytes, so every export is pixel-exact with no image library. The 16 px exports use the 16 px cut.
Each theme gets its own PNG folder and ICO.

    python3 design/brand/render.py
"""

import base64
import re
import struct
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
CHROMIUM = "chromium"
PNG_SIZES = [16, 24, 32, 48, 64, 128, 256, 512, 1024]
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]


THEMES = ["dark", "light"]


def source(theme, size):
    return HERE / (f"setu-icon-16-{theme}.svg" if size == 16 else f"setu-icon-{theme}.svg")


def render(theme, sizes):
    jobs = [[s, base64.b64encode(source(theme, s).read_bytes()).decode()] for s in sizes]
    page = f"""<!doctype html><body><pre id="out"></pre><script>
(async () => {{
  const out = [];
  for (const [size, b64] of {jobs}) {{
    const img = new Image();
    img.src = 'data:image/svg+xml;base64,' + b64;
    await img.decode();
    const c = document.createElement('canvas');
    c.width = c.height = size;
    c.getContext('2d').drawImage(img, 0, 0, size, size);
    out.push(size + ':' + c.toDataURL('image/png').split(',')[1]);
  }}
  document.getElementById('out').textContent = out.join('\\n');
}})();
</script></body>"""
    with tempfile.NamedTemporaryFile("w", suffix=".html", delete=False) as f:
        f.write(page)
    dom = subprocess.run(
        [CHROMIUM, "--headless=new", "--no-sandbox", "--virtual-time-budget=5000", "--dump-dom", f"file://{f.name}"],
        capture_output=True, text=True, check=True,
    ).stdout
    Path(f.name).unlink()
    return {int(s): base64.b64decode(b) for s, b in re.findall(r"(\d+):([A-Za-z0-9+/=]+)", dom)}


def ico(pngs):
    """An ICO holding PNG-compressed images, which Windows Vista and later read."""
    header = struct.pack("<HHH", 0, 1, len(pngs))
    offset = len(header) + 16 * len(pngs)
    entries, data = b"", b""
    for size, png in pngs:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(png), offset + len(data))
        data += png
    return header + entries + data


if __name__ == "__main__":
    for theme in THEMES:
        pngs = render(theme, PNG_SIZES)
        assert sorted(pngs) == PNG_SIZES, f"{theme}: rendered {sorted(pngs)}"
        out = HERE / "png" / theme
        out.mkdir(parents=True, exist_ok=True)
        for size, png in pngs.items():
            (out / f"setu-icon-{size}.png").write_bytes(png)
        (HERE / f"setu-{theme}.ico").write_bytes(ico([(s, pngs[s]) for s in ICO_SIZES]))
        print(f"png/{theme}/ ({len(pngs)} sizes), setu-{theme}.ico ({len(ICO_SIZES)} sizes)")
