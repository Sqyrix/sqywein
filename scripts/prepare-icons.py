"""Convert the supplied logo to the PNG and multi-resolution Windows ICO assets.

Usage: python scripts/prepare-icons.py [source.png]
Requires Pillow for this build-time conversion only; the browser does not use it.
"""
from pathlib import Path
import sys
from PIL import Image, ImageOps

root = Path(__file__).resolve().parent.parent
source = Path(sys.argv[1]) if len(sys.argv) > 1 else root / "Assets/logo-source.png"
with Image.open(source) as supplied:
    rgba = supplied.convert("RGBA")
    for size, name in [(256, "logo.png"), (512, "logo-512.png")]:
        scaled = ImageOps.contain(rgba, (size, size), Image.Resampling.LANCZOS)
        canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        canvas.alpha_composite(scaled, ((size - scaled.width) // 2, (size - scaled.height) // 2))
        canvas.save(root / "Assets" / name, optimize=True)
    canvas.save(root / "Assets/app.ico", format="ICO", sizes=[(n, n) for n in [16, 20, 24, 32, 40, 48, 64, 128, 256]])
print("Updated logo.png, logo-512.png and app.ico; aspect ratio and transparency preserved.")
