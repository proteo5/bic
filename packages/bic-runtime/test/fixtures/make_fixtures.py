"""Generate the conformance fixtures for the JavaScript runtime.

Each fixture is a .bic file made by the Rust encoder, plus the expected result
as the Rust decoder reports it: the RGBA hash for BIC's own coding, or the
stored file's hash and type when the container holds a PNG or an original.

The runtime tests need no Rust toolchain; they only read the committed
fixtures and manifest.json. Run this script again after changing the codec.

Usage (from the repository root, after `cargo build --release --examples`):
    python packages/bic-runtime/test/fixtures/make_fixtures.py
"""

import hashlib
import json
import os
import random
import subprocess
import sys

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
EXE = ".exe" if os.name == "nt" else ""
BIC = os.path.join(ROOT, "target", "release", "bic" + EXE)
DUMP = os.path.join(ROOT, "target", "release", "examples", "dump_rgba" + EXE)
SRC = os.path.join(HERE, "src")
OUT = os.path.join(HERE, "bic")


def run(*args):
    result = subprocess.run(args, capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"failed: {' '.join(args)}\n{result.stderr}")
    return result.stdout


def sources():
    """Small images that cover the encoder's paths."""
    rng = random.Random(7)
    images = {}

    chart = Image.new("RGBA", (120, 80), (255, 255, 255, 255))
    palette = [(13, 110, 253, 255), (25, 135, 84, 255), (220, 53, 69, 255)]
    for i, h in enumerate([50, 30, 64, 20, 44]):
        for x in range(10 + i * 20, 10 + i * 20 + 14):
            for y in range(80 - h, 79):
                chart.putpixel((x, y), palette[i % 3])
    images["chart"] = chart

    gradient = Image.new("RGBA", (96, 64))
    for x in range(96):
        for y in range(64):
            gradient.putpixel((x, y), ((x * 2) & 255, (y * 3) & 255, ((x + y) // 2) & 255, 255))
    images["gradient"] = gradient

    noise = Image.new("RGBA", (64, 48))
    for x in range(64):
        for y in range(48):
            noise.putpixel((x, y), (rng.randrange(256), rng.randrange(256), rng.randrange(256), 255))
    images["noise"] = noise

    pixel = Image.new("RGBA", (32, 24))
    colors = [(20, 12, 28, 255), (68, 36, 52, 255), (48, 52, 109, 255), (0, 0, 0, 0),
              (133, 76, 48, 255), (52, 101, 36, 255), (208, 70, 72, 255), (109, 170, 44, 255)]
    for x in range(32):
        for y in range(24):
            pixel.putpixel((x, y), colors[rng.randrange(len(colors))])
    images["pixel-art"] = pixel

    icon = Image.new("RGBA", (48, 48))
    for x in range(48):
        for y in range(48):
            d = ((x - 23.5) ** 2 + (y - 23.5) ** 2) ** 0.5
            alpha = max(0, min(255, int((20 - abs(d - 18)) * 12)))
            icon.putpixel((x, y), (13, 110, 253, alpha))
    images["icon-alpha"] = icon

    odd = Image.new("RGBA", (7, 5))
    for x in range(7):
        for y in range(5):
            odd.putpixel((x, y), (rng.randrange(256), x * 30, y * 50, 255))
    images["odd-size"] = odd
    images["one-pixel"] = Image.new("RGBA", (1, 1), (9, 80, 200, 255))
    images["one-row"] = Image.new("RGBA", (9, 1), (1, 2, 3, 255))
    images["one-column"] = Image.new("RGBA", (1, 9), (4, 5, 6, 200))

    photo = Image.new("RGB", (160, 100))
    for x in range(160):
        for y in range(100):
            n = rng.randrange(24)
            photo.putpixel((x, y), ((x + n) & 255, (y * 2 + n) & 255, (x + y + n) & 255))
    images["photo"] = photo
    return images


# Encoder settings tried for every source. Each one exercises another part of
# the format: lossy quantization, the vectorizable decode path, and the PNG
# fallback container.
VARIANTS = {
    "lossless": [],
    "fast": ["--fast-decode"],
    "lossy2": ["--max-error", "2"],
    "lossy5-fast": ["--max-error", "5", "--fast-decode"],
    "png-fallback": ["--png-fallback"],
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    if not (os.path.exists(BIC) and os.path.exists(DUMP)):
        sys.exit("build first: cargo build --release --examples")
    os.makedirs(SRC, exist_ok=True)
    os.makedirs(OUT, exist_ok=True)
    for old in os.listdir(OUT):
        os.remove(os.path.join(OUT, old))

    manifest = []
    for name, image in sources().items():
        png = os.path.join(SRC, name + ".png")
        image.save(png, "PNG")
        for variant, args in VARIANTS.items():
            fixture = f"{name}--{variant}"
            bic = os.path.join(OUT, fixture + ".bic")
            stdout = run(BIC, "encode", png, bic, "--zstd-level", "3", *args)
            manifest.append(describe(fixture, bic, png, stdout))

    # An original JPEG kept inside the container, and the same photo transcoded.
    jpeg = os.path.join(SRC, "photo.jpg")
    Image.open(os.path.join(SRC, "photo.png")).convert("RGB").save(jpeg, "JPEG", quality=90)
    bic = os.path.join(OUT, "photo-jpeg--original.bic")
    stdout = run(BIC, "encode", jpeg, bic, "--zstd-level", "3")
    manifest.append(describe("photo-jpeg--original", bic, jpeg, stdout))
    bic = os.path.join(OUT, "photo-jpeg--transcode.bic")
    stdout = run(BIC, "encode", jpeg, bic, "--zstd-level", "3", "--transcode")
    manifest.append(describe("photo-jpeg--transcode", bic, jpeg, stdout))

    # A container that stores a PNG of the pixels (storage 1). The encoder only
    # picks this when it is smaller, which these small images rarely are, so the
    # container is written by hand (FORMAT.md, "Container").
    png = os.path.join(SRC, "gradient.png")
    payload = open(png, "rb").read()
    header = b"BIC2" + bytes([2, 1, 0, 0]) + (96).to_bytes(4, "little") + (64).to_bytes(4, "little")
    header += len(payload).to_bytes(8, "little") + len(payload).to_bytes(8, "little")
    bic = os.path.join(OUT, "gradient--stored-png.bic")
    with open(bic, "wb") as f:
        f.write(header + payload)
    entry = describe("gradient--stored-png", bic, png, "PNG fallback")
    manifest.append(entry)

    with open(os.path.join(HERE, "manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=1)
    print(f"{len(manifest)} fixtures written")


def describe(fixture, bic, source, encode_stdout):
    """The expected result of one fixture, as the Rust decoder reports it."""
    if "original file kept" in encode_stdout:
        kind = "file"
    elif "PNG fallback" in encode_stdout:
        kind = "file"
    else:
        kind = "pixels"
    with open(bic, "rb") as f:
        data = f.read()
    entry = {"id": fixture, "file": os.path.basename(bic), "kind": kind, "bic_bytes": len(data)}
    if kind == "file":
        with open(source, "rb") as f:
            stored = f.read()
        # The stored file is the source itself (original) or a PNG of the pixels.
        if "PNG fallback" in encode_stdout:
            entry["mime"] = "image/png"
            entry["sha256"] = None  # checked through the decoded pixels below
        else:
            entry["mime"] = "image/jpeg" if source.endswith(".jpg") else "image/png"
            entry["sha256"] = sha(stored)
    raw = bic + ".raw"
    run(DUMP, bic, raw)
    with open(raw, "rb") as f:
        dumped = f.read()
    os.remove(raw)
    width = int.from_bytes(dumped[0:4], "little")
    height = int.from_bytes(dumped[4:8], "little")
    entry["width"] = width
    entry["height"] = height
    entry["rgba_sha256"] = sha(dumped[8:])
    return entry


if __name__ == "__main__":
    main()
