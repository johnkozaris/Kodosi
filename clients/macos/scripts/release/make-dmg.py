#!/usr/bin/env python3
"""Makes the unsigned Kodosi disk image: one APFS volume named Kodosi, compressed with LZFSE, that
holds the app and a link to Applications. Its window opens at a fixed size in icon view, without
toolbar, sidebar or status bar, on the background picture beside this script: the app on the left,
an arrow, and Applications on the right. The layout goes into the volume's .DS_Store without Finder,
so no window opens while the image is made. With --background, draws that picture again; this needs
only the standard library and tiffutil."""

from __future__ import annotations

import argparse
import math
import plistlib
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
BACKGROUND = HERE / "dmg-background.tiff"
VOLUME_NAME = "Kodosi"
APP_NAME = "Kodosi.app"
BACKGROUND_NAME = ".background.tiff"
# Finder's window bounds include the title bar, and Finder draws the picture from the top of the
# window's content. The picture is as tall as the whole window, so the title bar hides only its
# bottom edge, which is the plain surface.
WINDOW = (600, 360)
WINDOW_ORIGIN = (440, 320)
ICON_SIZE = 128
TEXT_SIZE = 13
ROW = 150
APP_CENTER = (150, ROW)
APPLICATIONS_CENTER = (450, ROW)
# A warm surface on which Finder's black names in the light look and white names in the dark look
# both have a contrast of 4.6:1.
SURFACE = (0x8B, 0x70, 0x60)
ARROW_INK = (0x60, 0x4C, 0x41)
ARROW_WIDTH = 3.0
# Half a point below the icons' centres, the arrow's line covers whole pixels at both scales.
ARROW_ROW = ROW + 0.5
ARROW_LENGTH = 52.0
ARROW_HEAD = 11.0


def run(*command: str | Path) -> bytes:
    return subprocess.run(command, check=True, stdout=subprocess.PIPE).stdout


def arrow_strokes() -> list[tuple[float, float, float, float]]:
    middle = (APP_CENTER[0] + APPLICATIONS_CENTER[0]) / 2
    tail, tip = middle - ARROW_LENGTH / 2, middle + ARROW_LENGTH / 2
    return [
        (tail, ARROW_ROW, tip, ARROW_ROW),
        (tip - ARROW_HEAD, ARROW_ROW - ARROW_HEAD, tip, ARROW_ROW),
        (tip - ARROW_HEAD, ARROW_ROW + ARROW_HEAD, tip, ARROW_ROW),
    ]


def distance_to_stroke(x: float, y: float, stroke: tuple[float, float, float, float]) -> float:
    ax, ay, bx, by = stroke
    dx, dy = bx - ax, by - ay
    along = max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(x - ax - along * dx, y - ay - along * dy)


def background_rows(scale: int) -> list[bytes]:
    """The picture at a scale, as rows of RGB pixels: the surface, and the arrow with round ends,
    its edges covered in proportion to each pixel."""
    width, height = WINDOW[0] * scale, WINDOW[1] * scale
    strokes = arrow_strokes()
    reach = ARROW_WIDTH
    left = math.floor((min(min(s[0], s[2]) for s in strokes) - reach) * scale)
    right = math.ceil((max(max(s[0], s[2]) for s in strokes) + reach) * scale)
    top = math.floor((min(min(s[1], s[3]) for s in strokes) - reach) * scale)
    bottom = math.ceil((max(max(s[1], s[3]) for s in strokes) + reach) * scale)
    plain = bytes(SURFACE) * width
    rows = []
    for row in range(height):
        if not top <= row < bottom:
            rows.append(plain)
            continue
        pixels = bytearray(plain)
        for column in range(left, right):
            x, y = (column + 0.5) / scale, (row + 0.5) / scale
            edge = min(distance_to_stroke(x, y, stroke) for stroke in strokes) - ARROW_WIDTH / 2
            cover = max(0.0, min(1.0, 0.5 - edge * scale))
            if cover:
                pixels[column * 3 : column * 3 + 3] = bytes(
                    round(surface + (ink - surface) * cover) for surface, ink in zip(SURFACE, ARROW_INK)
                )
        rows.append(bytes(pixels))
    return rows


def write_png(path: Path, scale: int) -> None:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    width, height = WINDOW[0] * scale, WINDOW[1] * scale
    pixels_per_metre = round(72 * scale / 0.0254)
    data = b"".join(b"\0" + row for row in background_rows(scale))
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"sRGB", b"\0")
        + chunk(b"pHYs", struct.pack(">IIB", pixels_per_metre, pixels_per_metre, 1))
        + chunk(b"IDAT", zlib.compress(data, 9))
        + chunk(b"IEND", b"")
    )


def draw_background() -> None:
    """Writes the picture at the window's size for normal and Retina displays into one TIFF."""
    with tempfile.TemporaryDirectory() as folder:
        normal, retina = Path(folder) / "background.png", Path(folder) / "background@2x.png"
        write_png(normal, 1)
        write_png(retina, 2)
        joined = Path(folder) / "background.tiff"
        run("/usr/bin/tiffutil", "-cathidpicheck", normal, retina, "-out", joined)
        run("/usr/bin/tiffutil", "-lzw", joined, "-out", BACKGROUND)


def write_layout(volume: Path) -> None:
    from ds_store import DSStore
    from mac_alias import Alias

    background = Alias.for_file(str(volume / BACKGROUND_NAME))
    red, green, blue = (channel / 255 for channel in SURFACE)
    (x, y), (width, height) = WINDOW_ORIGIN, WINDOW
    with DSStore.open(str(volume / ".DS_Store"), "w+") as store:
        store["."]["vSrn"] = ("long", 1)
        store["."]["icvl"] = ("type", b"icnv")
        store["."]["bwsp"] = {
            "WindowBounds": f"{{{{{x}, {y}}}, {{{width}, {height}}}}}",
            "ShowToolbar": False,
            "ShowSidebar": False,
            "ContainerShowSidebar": False,
            "SidebarWidth": 180,
            "ShowStatusBar": False,
            "ShowPathbar": False,
            "ShowTabView": False,
            "PreviewPaneVisibility": False,
        }
        store["."]["icvp"] = {
            "viewOptionsVersion": 1,
            "backgroundType": 2,
            "backgroundImageAlias": background.to_bytes(),
            "backgroundColorRed": red,
            "backgroundColorGreen": green,
            "backgroundColorBlue": blue,
            "arrangeBy": "none",
            "gridOffsetX": 0.0,
            "gridOffsetY": 0.0,
            "gridSpacing": 100.0,
            "iconSize": float(ICON_SIZE),
            "textSize": float(TEXT_SIZE),
            "labelOnBottom": True,
            "showIconPreview": False,
            "showItemInfo": False,
            "scrollPositionX": 0.0,
            "scrollPositionY": 0.0,
        }
        store[APP_NAME]["Iloc"] = APP_CENTER
        store["Applications"]["Iloc"] = APPLICATIONS_CENTER


def make_image(app: Path, dmg: Path) -> None:
    """Fills a writable image from a staged folder, writes the layout on its volume, which is
    mounted hidden from Finder, and compresses it into the disk image. The layout points at the
    background on that volume, which the compressed image keeps as it is."""
    with tempfile.TemporaryDirectory(prefix="kodosi-dmg.") as folder:
        staging = Path(folder) / VOLUME_NAME
        staging.mkdir()
        run("/usr/bin/ditto", app, staging / APP_NAME)
        (staging / "Applications").symlink_to("/Applications")
        shutil.copyfile(BACKGROUND, staging / BACKGROUND_NAME)
        writable = Path(folder) / "writable.dmg"
        run("diskutil", "image", "create", "from", "--format", "RAW", "--volumeName", VOLUME_NAME, staging, writable)
        entities = plistlib.loads(run("diskutil", "image", "attach", "--plist", "--nobrowse", writable))["system-entities"]
        try:
            write_layout(next(Path(entity["mount-point"]) for entity in entities if "mount-point" in entity))
        finally:
            run("diskutil", "eject", entities[0]["dev-entry"])
        dmg.parent.mkdir(parents=True, exist_ok=True)
        run("diskutil", "image", "create", "from", "--format", "ULFO", writable, dmg)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--background", action="store_true", help=f"draw {BACKGROUND.name} again")
    parser.add_argument("app", nargs="?", type=Path)
    parser.add_argument("dmg", nargs="?", type=Path)
    arguments = parser.parse_args()
    if arguments.background:
        draw_background()
    elif arguments.app and arguments.dmg:
        make_image(arguments.app, arguments.dmg)
    else:
        parser.error("give the app and the disk image, or --background")


if __name__ == "__main__":
    sys.exit(main())
