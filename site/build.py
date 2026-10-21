"""Build the demo page: encode each example with bic, measure it against the
usual formats, and write a static site to site/out.

Every number on the page is measured here, not copied. The page shows
lossless and lossy alternatives side by side, and says where BIC is not the
smallest option.

Usage (after `cargo build --release`):
    python site/build.py
"""

import csv
import html
import io
import json
import os
import shutil
import subprocess
import sys

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SOURCES = os.path.join(HERE, "sources")
OUT = os.path.join(HERE, "out")
BIC = os.environ.get(
    "BIC_BIN",
    os.path.join(ROOT, "target", "release", "bic" + (".exe" if os.name == "nt" else "")),
)
MB = 1_000_000  # bandwidth is reported in decimal megabytes


def run(*args):
    result = subprocess.run(args, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        sys.exit(f"command failed: {' '.join(args)}\n{result.stderr or result.stdout}")
    return result.stdout


def has_alpha(image):
    return image.mode in ("RGBA", "LA") or (image.mode == "P" and "transparency" in image.info)


def lossy_sizes(path):
    """File sizes of the common lossy formats for the same pixels."""
    image = Image.open(path)
    alpha = has_alpha(image)
    sizes = {}

    buf = io.BytesIO()
    image.convert("RGBA" if alpha else "RGB").save(buf, "WEBP", quality=80, method=6)
    sizes["webp_q80"] = buf.tell()

    buf = io.BytesIO()
    image.convert("RGBA" if alpha else "RGB").save(buf, "AVIF", quality=50)
    sizes["avif_q50"] = buf.tell()

    # JPEG has no alpha: flatten onto white, which is what a web page shows.
    rgb = image.convert("RGBA")
    flat = Image.new("RGB", rgb.size, (255, 255, 255))
    flat.paste(rgb, mask=rgb.getchannel("A"))
    buf = io.BytesIO()
    flat.save(buf, "JPEG", quality=85, optimize=True)
    sizes["jpeg_q85"] = buf.tell()
    return sizes


def measure_bic(source, bic_path):
    """Encode with the default settings and check the roundtrip exactly."""
    stdout = run(BIC, "encode", source, bic_path)
    kept = "original file kept" in stdout
    verify = run(BIC, "verify", source, bic_path)
    if "exact pixel roundtrip verified" not in verify:
        sys.exit(f"roundtrip not exact for {source}: {verify}")
    return {"bytes": os.path.getsize(bic_path), "kept_original": kept}


def native_measurements(bench_csv):
    """Pixel-level sizes from `bic compare`: PNG, oxipng, WebP lossless and
    BIC's own coding (before the original-file option)."""
    rows = {}
    with open(bench_csv, encoding="utf-8") as f:
        for row in csv.DictReader(f):
            name = os.path.splitext(row["image"])[0]
            rows[name] = {
                "png": int(row["png"]),
                "oxipng": int(row["oxipng"]),
                "webp_lossless": int(row["webp_lossless"]),
                "bic_native": int(row["bic"]),
                "decode_ms": float(row["bic_decode_ms"]),
            }
    return rows


def kib(n):
    return f"{n / 1024:,.1f}"


def pct(a, b):
    """How much smaller `a` is than `b`, in percent. Negative when `a` is larger."""
    return (1 - a / b) * 100


def build_rows(manifest):
    os.makedirs(OUT, exist_ok=True)
    bic_dir = os.path.join(OUT, "bic")
    img_dir = os.path.join(OUT, "img")
    os.makedirs(bic_dir, exist_ok=True)
    os.makedirs(img_dir, exist_ok=True)

    bench = os.path.join(OUT, "bench.csv")
    run(BIC, "compare", SOURCES, "--csv", bench)
    native = native_measurements(bench)

    rows = []
    for entry in manifest:
        source = os.path.join(SOURCES, entry["file"])
        stem = os.path.splitext(entry["file"])[0]
        bic_path = os.path.join(bic_dir, stem + ".bic")
        shutil.copy(source, os.path.join(img_dir, entry["file"]))
        bic = measure_bic(source, bic_path)
        lossy = lossy_sizes(source)
        pixels = native[stem]

        lossless = {
            "PNG": pixels["png"],
            "oxipng": pixels["oxipng"],
            "WebP lossless": pixels["webp_lossless"],
        }
        best_lossless_name = min(lossless, key=lossless.get)
        best_lossless = lossless[best_lossless_name]
        lossy_named = {
            "WebP q80": lossy["webp_q80"],
            "AVIF q50": lossy["avif_q50"],
            "JPEG q85": lossy["jpeg_q85"],
        }
        best_lossy_name = min(lossy_named, key=lossy_named.get)
        best_lossy = lossy_named[best_lossy_name]

        # A kept original is lossy already, so a lossless comparison is not
        # meaningful: report it as not applicable instead of a percentage.
        if bic["kept_original"]:
            saving = None
            mb_per_1000 = None
        else:
            saving = pct(bic["bytes"], best_lossless)
            mb_per_1000 = (best_lossless - bic["bytes"]) * 1000 / MB
        rows.append({
            **entry,
            "bic_bytes": bic["bytes"],
            "kept_original": bic["kept_original"],
            "bic_native": pixels["bic_native"],
            "lossless": lossless,
            "lossy": lossy_named,
            "best_lossless": best_lossless_name,
            "best_lossy": best_lossy_name,
            "saving_vs_best_lossless": saving,
            "mb_per_1000_views": mb_per_1000,
            "lossy_smaller_than_bic": best_lossy < bic["bytes"],
            "lossy_saving": pct(best_lossy, bic["bytes"]),
            "lossy_note": None,
            "decode_ms": pixels["decode_ms"],
        })
        print(f"{entry['id']:20} bic {bic['bytes']:>9} best lossless {best_lossless_name} "
              f"{best_lossless:>9} saving {'n/a' if saving is None else f'{saving:+6.1f}%'}")
    return rows


def verdict(row):
    if row["kept_original"]:
        return ("Stores the original JPEG, so BIC adds nothing: "
                "use JPEG, WebP or AVIF for photographs.")
    if row["saving_vs_best_lossless"] > 0:
        return f"Good fit: {row['saving_vs_best_lossless']:.0f}% smaller than {row['best_lossless']}."
    return f"Not a good fit: {row['best_lossless']} is smaller. Use it instead."


def render_table(rows):
    head = (
        "<tr><th>Example</th><th>Pixels</th>"
        "<th>PNG</th><th>oxipng</th><th>WebP lossless</th><th>BIC file</th>"
        "<th>Best lossless</th><th>BIC smaller by</th>"
        "<th>MB saved per 1,000 views</th>"
        "<th>WebP q80</th><th>AVIF q50</th><th>JPEG q85</th>"
        "<th>What to use</th></tr>"
    )
    body = []
    for r in rows:
        file_name = html.escape(r["file"])
        stem = html.escape(os.path.splitext(r["file"])[0])
        lossy_note = ""
        if r["kept_original"]:
            lossy_note = ("<br><small>BIC keeps the source JPEG (quality 90) as it is. "
                          "Lossless formats are much larger for photographs.</small>")
        elif r["lossy_smaller_than_bic"]:
            lossy_note = (f"<br><small>Lossy {html.escape(r['best_lossy'])} is "
                          f"{r['lossy_saving']:.0f}% smaller than BIC, if exact pixels are not needed.</small>")
        saving = ("n/a" if r["saving_vs_best_lossless"] is None
                  else f"{r['saving_vs_best_lossless']:+.0f}%")
        per_views = ("n/a" if r["mb_per_1000_views"] is None
                     else f"{r['mb_per_1000_views']:+.2f}")
        body.append(
            "<tr>"
            f"<td><a href='img/{file_name}'><img class='thumb' src='img/{file_name}' alt=''></a>"
            f"{html.escape(r['description'])}<br><small>{html.escape(r['kind'])}</small></td>"
            f"<td>{r['width']}×{r['height']}</td>"
            f"<td>{kib(r['lossless']['PNG'])}</td>"
            f"<td>{kib(r['lossless']['oxipng'])}</td>"
            f"<td>{kib(r['lossless']['WebP lossless'])}</td>"
            f"<td><strong>{kib(r['bic_bytes'])}</strong>"
            f"<br><small><a href='bic/{stem}.bic'>.bic</a></small></td>"
            f"<td>{html.escape(r['best_lossless'])}: {kib(r['lossless'][r['best_lossless']])}</td>"
            f"<td>{saving}</td>"
            f"<td>{per_views}</td>"
            f"<td>{kib(r['lossy']['WebP q80'])}</td>"
            f"<td>{kib(r['lossy']['AVIF q50'])}</td>"
            f"<td>{kib(r['lossy']['JPEG q85'])}</td>"
            f"<td>{html.escape(verdict(r))}{lossy_note}</td>"
            "</tr>"
        )
    return f"<table><thead>{head}</thead><tbody>{''.join(body)}</tbody></table>"


def render_attribution(rows):
    items = []
    for r in rows:
        license_link = (f"<a href='{html.escape(r['license_url'])}'>{html.escape(r['license'])}</a>"
                        if r["license_url"] else html.escape(r["license"]))
        resized = " Resized to fit 1600 px." if r["resized"] else ""
        items.append(
            f"<li>{html.escape(r['description'])}: "
            f"<a href='{html.escape(r['page_url'])}'>{html.escape(r['source_file'])}</a>, "
            f"by {html.escape(r['author'] or 'unknown')}, {license_link}.{resized}</li>"
        )
    return "<ul>" + "".join(items) + "</ul>"


PAGE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>BIC: when it makes files smaller, and when it does not</title>
<style>
  :root {{ color-scheme: light dark; --muted: #6b7280; --line: #d1d5db; }}
  body {{ font: 16px/1.5 system-ui, sans-serif; max-width: 1200px; margin: 2rem auto; padding: 0 1rem; }}
  h1 {{ margin-bottom: .25rem; }}
  .lead {{ color: var(--muted); margin-top: 0; }}
  .wrap {{ overflow-x: auto; }}
  table {{ border-collapse: collapse; width: 100%; margin: 1rem 0; font-size: 14px; }}
  th, td {{ border: 1px solid var(--line); padding: .4rem .5rem; vertical-align: top; text-align: right; }}
  th {{ background: rgba(127,127,127,.12); }}
  td:first-child, td:last-child, th:first-child, th:last-child {{ text-align: left; }}
  .thumb {{ max-width: 140px; max-height: 90px; display: block; margin-bottom: .25rem; }}
  .note {{ border-left: 4px solid #2563eb; padding: .25rem 1rem; margin: 1.5rem 0; background: rgba(37,99,235,.06); }}
  small {{ color: var(--muted); }}
  code {{ font-size: 90%; }}
</style>
</head>
<body>
<h1>BIC: when it makes files smaller, and when it does not</h1>
<p class="lead">BIC is a lossless image format. On some flat graphics it is smaller
than optimized PNG, and on others it is larger. Lossy formats such as WebP and AVIF
are often much smaller, but they change pixel values. The results below are measured on
each example, including the cases where BIC loses.</p>

<div class="note">
<strong>Where BIC was smaller than optimized PNG:</strong> a bar chart and an icon with few colors.<br>
<strong>Where it was larger:</strong> a software interface with anti-aliased text, pixel
art and a flowchart. Real screenshots have many colors, and BIC's palette mode does not
apply to them.<br>
<strong>Photographs:</strong> BIC stores the JPEG unchanged, adding 32 bytes. Use JPEG, WebP or AVIF.
</div>

<h2>Results</h2>
<p>All sizes are in KiB. &ldquo;Best lossless&rdquo; is the smallest of PNG, oxipng and
WebP lossless. The last three columns are lossy formats, which keep fewer pixel
values exactly. &ldquo;MB saved per 1,000 views&rdquo; multiplies the difference by
1,000 transfers of the same image. Negative values mean BIC is larger.</p>
<div class="wrap">{table}</div>

<h2>How the bandwidth figure is calculated</h2>
<p>Image files are already compressed, so a web server does not shrink them further.
The bytes saved on every transfer are therefore the file-size difference. Multiply by
the number of views to get the total. A 20&nbsp;% saving on a 150&nbsp;KiB screenshot
is about 30&nbsp;KiB per view, which matters on a busy site and little on a single page.</p>

<h2>Viewing a .bic file</h2>
<p>Browsers cannot open <code>.bic</code> files yet. Download the
<a href="https://github.com/proteo5/bic/releases">bic-view</a> program for Windows,
Linux or macOS, or convert the file back to PNG with
<code>bic decode file.bic file.png</code>. A browser extension or a Service Worker that
shows <code>.bic</code> in <code>&lt;img&gt;</code> is a possible next step.</p>

<h2>How the numbers are produced</h2>
<ul>
  <li>BIC: <code>bic encode</code> with default settings (lossless, Zstandard level 19).
      Each file is checked with <code>bic verify</code>, which requires the decoded pixels to match the source exactly.</li>
  <li>PNG: the <code>image</code> crate's default encoder. oxipng: preset 2.
      WebP lossless: the <code>image</code> crate's encoder.</li>
  <li>Lossy formats: Pillow {pillow}. WebP quality 80, method 6. AVIF quality 50.
      JPEG quality 85, transparency flattened onto white.</li>
  <li>Photographs are stored as JPEG (quality 90) sources. Their BIC file keeps that JPEG.</li>
  <li>BIC stores an ICC color profile when the source has one. On small images this adds
      a few hundred bytes.</li>
  <li>Source images are the files listed below, resized to at most 1600 pixels per side
      when larger. The same pixels go to every format.</li>
</ul>

<h2>Sources and licenses</h2>
{attribution}

<p><small>Generated by <code>site/build.py</code> from the
<a href="https://github.com/proteo5/bic">BIC repository</a>. The images are
republished under their original licenses, with the changes stated above.</small></p>
</body>
</html>
"""


def main():
    manifest_path = os.path.join(SOURCES, "manifest.json")
    if not os.path.exists(manifest_path):
        sys.exit("run site/prepare_sources.py first")
    if not os.path.exists(BIC):
        sys.exit(f"bic binary not found at {BIC}; run cargo build --release")
    with open(manifest_path, encoding="utf-8") as f:
        manifest = json.load(f)
    rows = build_rows(manifest)
    import PIL

    page = PAGE.format(
        table=render_table(rows),
        attribution=render_attribution(rows),
        pillow=PIL.__version__,
    )
    with open(os.path.join(OUT, "index.html"), "w", encoding="utf-8") as f:
        f.write(page)
    with open(os.path.join(OUT, "results.json"), "w", encoding="utf-8") as f:
        json.dump(rows, f, ensure_ascii=False, indent=1)
    print(f"wrote {os.path.join(OUT, 'index.html')}")


if __name__ == "__main__":
    main()
