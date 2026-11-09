"""Convert the supplied logo to the PNG and multi-resolution Windows ICO assets.

Usage: python scripts/prepare-icons.py [source.png]
Requires Pillow for this build-time conversion only; the browser does not use it.
Transparent margins are trimmed so the logo fills the icon (sharper at 16-32 px).
"""
from pathlib import Path
import sys
from PIL import Image, ImageFilter, ImageOps

root = Path(__file__).resolve().parent.parent
source = Path(sys.argv[1]) if len(sys.argv) > 1 else root / "Assets/logo-source.png"


def square(rgba: Image.Image, size: int, pad: float) -> Image.Image:
    inner = max(1, round(size * (1 - 2 * pad)))
    scaled = ImageOps.contain(rgba, (inner, inner), Image.Resampling.LANCZOS)
    if size <= 48:
        # Küçük boyutlarda kenarlar yumuşamasın.
        scaled = scaled.filter(ImageFilter.UnsharpMask(radius=0.6, percent=60, threshold=0))
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas.alpha_composite(scaled, ((size - scaled.width) // 2, (size - scaled.height) // 2))
    return canvas


with Image.open(source) as supplied:
    rgba = supplied.convert("RGBA")
    box = rgba.getchannel("A").point(lambda a: 255 if a > 8 else 0).getbbox()
    if box:
        rgba = rgba.crop(box)
    for size, name in [(256, "logo.png"), (512, "logo-512.png")]:
        square(rgba, size, 0.04).save(root / "Assets" / name, optimize=True)
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    frames = [square(rgba, n, 0.02 if n <= 32 else 0.04) for n in sizes]
    frames[-1].save(root / "Assets/app.ico", format="ICO", sizes=[(n, n) for n in sizes], append_images=frames[:-1])
print("Updated logo.png, logo-512.png and app.ico; margins trimmed, transparency preserved.")
