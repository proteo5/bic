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


# ---------------------------------------------------------------------------
# Rendering. English is the primary language (/), Spanish is /es/. Both pages
# show the same measurements; only the text changes.
# ---------------------------------------------------------------------------

DESCRIPTIONS_ES = {
    "ui-screenshot": "Interfaz de software (captura)",
    "flowchart": "Diagrama de flujo",
    "bar-chart": "Gráfica de barras",
    "icon": "Ícono con transparencia",
    "pixel-art": "Pixel art",
    "photo-landscape-1": "Fotografía de paisaje",
    "photo-landscape-2": "Fotografía de paisaje",
}

STRINGS = {
    "en": {
        "lang_code": "en",
        "switch_href": "es/",
        "switch_label": "Español",
        "title": "BIC: when it makes files smaller, and when it does not",
        "h1": "BIC: when it makes files smaller, and when it does not",
        "lead": ("BIC is a lossless image format. On some flat graphics it is smaller "
                 "than optimized PNG, and on others it is larger. Lossy formats such as "
                 "WebP and AVIF are often much smaller, but they change pixel values. "
                 "The results below are measured on each example, including the cases "
                 "where BIC loses."),
        "note_win": "Where BIC was smaller than optimized PNG:",
        "note_win_text": "a bar chart and an icon with few colors.",
        "note_lose": "Where it was larger:",
        "note_lose_text": ("a software interface with anti-aliased text, pixel art and a "
                           "flowchart. Real screenshots have many colors, and BIC's palette "
                           "mode does not apply to them."),
        "note_photo": "Photographs:",
        "note_photo_text": "BIC stores the JPEG unchanged, adding 32 bytes. Use JPEG, WebP or AVIF.",
        "h_results": "Results",
        "results_intro": ("All sizes are in KiB. &ldquo;Best lossless&rdquo; is the smallest of "
                          "PNG, oxipng and WebP lossless. The last three columns are lossy "
                          "formats, which keep fewer pixel values exactly. &ldquo;MB saved per "
                          "1,000 views&rdquo; multiplies the difference by 1,000 transfers of the "
                          "same image. Negative values mean BIC is larger."),
        "th": ["Example", "Pixels", "PNG", "oxipng", "WebP lossless", "BIC file",
               "Best lossless", "BIC smaller by", "MB saved per 1,000 views",
               "WebP q80", "AVIF q50", "JPEG q85", "What to use"],
        "kind": {"flat": "flat graphics", "photo": "photograph"},
        "na": "n/a",
        "lossy_photo_note": ("BIC keeps the source JPEG (quality 90) as it is. "
                             "Lossless formats are much larger for photographs."),
        "lossy_note": "Lossy {fmt} is {pct}% smaller than BIC, if exact pixels are not needed.",
        "verdict_photo": "Stores the original JPEG, so BIC adds nothing: use JPEG, WebP or AVIF for photographs.",
        "verdict_good": "Good fit: {pct}% smaller than {best}.",
        "verdict_bad": "Not a good fit: {best} is smaller. Use it instead.",
        "h_bandwidth": "How the bandwidth figure is calculated",
        "bandwidth": ("Image files are already compressed, so a web server does not shrink them "
                      "further. The bytes saved on every transfer are therefore the file-size "
                      "difference. Multiply by the number of views to get the total. A 20&nbsp;% "
                      "saving on a 150&nbsp;KiB screenshot is about 30&nbsp;KiB per view, which "
                      "matters on a busy site and little on a single page."),
        "h_view": "Viewing a .bic file",
        "view": ("Browsers cannot open <code>.bic</code> files yet. Download the "
                 "<a href=\"https://github.com/proteo5/bic/releases\">bic-view</a> program for "
                 "Windows, Linux or macOS, or convert the file back to PNG with "
                 "<code>bic decode file.bic file.png</code>. A browser extension or a Service "
                 "Worker that shows <code>.bic</code> in <code>&lt;img&gt;</code> is a possible "
                 "next step."),
        "h_method": "How the numbers are produced",
        "method": [
            "BIC: <code>bic encode</code> with default settings (lossless, Zstandard level 19). "
            "Each file is checked with <code>bic verify</code>, which requires the decoded pixels "
            "to match the source exactly.",
            "PNG: the <code>image</code> crate's default encoder. oxipng: preset 2. "
            "WebP lossless: the <code>image</code> crate's encoder.",
            "Lossy formats: Pillow {pillow}. WebP quality 80, method 6. AVIF quality 50. "
            "JPEG quality 85, transparency flattened onto white.",
            "Photographs are stored as JPEG (quality 90) sources. Their BIC file keeps that JPEG.",
            "BIC stores an ICC color profile when the source has one. On small images this adds "
            "a few hundred bytes.",
            "Source images are the files listed below, resized to at most 1600 pixels per side "
            "when larger. The same pixels go to every format.",
        ],
        "h_sources": "Sources and licenses",
        "by": "by",
        "unknown": "unknown",
        "resized": "Resized to fit 1600 px.",
        "footer": ("Generated by <code>site/build.py</code> from the "
                   "<a href=\"https://github.com/proteo5/bic\">BIC repository</a>. The images are "
                   "republished under their original licenses, with the changes stated above."),
    },
    "es": {
        "lang_code": "es",
        "switch_href": "../",
        "switch_label": "English",
        "title": "BIC: cuándo hace los archivos más pequeños y cuándo no",
        "h1": "BIC: cuándo hace los archivos más pequeños y cuándo no",
        "lead": ("BIC es un formato de imagen sin pérdida. En algunos gráficos planos ocupa menos "
                 "que un PNG optimizado, y en otros ocupa más. Los formatos con pérdida como WebP y "
                 "AVIF suelen ser mucho más pequeños, pero cambian los valores de los píxeles. Los "
                 "resultados de abajo se midieron en cada ejemplo, incluidos los casos en que BIC pierde."),
        "note_win": "Donde BIC fue más pequeño que un PNG optimizado:",
        "note_win_text": "una gráfica de barras y un ícono con pocos colores.",
        "note_lose": "Donde fue más grande:",
        "note_lose_text": ("una interfaz de software con texto suavizado, pixel art y un diagrama de "
                           "flujo. Las capturas reales tienen muchos colores, y el modo de paleta de "
                           "BIC no se aplica a ellas."),
        "note_photo": "Fotografías:",
        "note_photo_text": "BIC guarda el JPEG sin cambios y agrega 32 bytes. Usa JPEG, WebP o AVIF.",
        "h_results": "Resultados",
        "results_intro": ("Todos los tamaños están en KiB. &ldquo;Mejor sin pérdida&rdquo; es el más "
                          "pequeño de PNG, oxipng y WebP sin pérdida. Las últimas tres columnas son "
                          "formatos con pérdida, que conservan menos valores de píxel exactos. &ldquo;MB "
                          "ahorrados por cada 1,000 vistas&rdquo; multiplica la diferencia por 1,000 "
                          "transferencias de la misma imagen. Los valores negativos significan que BIC "
                          "es más grande."),
        "th": ["Ejemplo", "Píxeles", "PNG", "oxipng", "WebP sin pérdida", "Archivo BIC",
               "Mejor sin pérdida", "BIC más pequeño por", "MB ahorrados por cada 1,000 vistas",
               "WebP q80", "AVIF q50", "JPEG q85", "Qué usar"],
        "kind": {"flat": "gráficos planos", "photo": "fotografía"},
        "na": "n/d",
        "lossy_photo_note": ("BIC conserva el JPEG original (calidad 90) tal cual. Los formatos sin "
                             "pérdida son mucho más grandes para fotografías."),
        "lossy_note": "{fmt} con pérdida es {pct}% más pequeño que BIC, si no se necesitan píxeles exactos.",
        "verdict_photo": "Guarda el JPEG original, así que BIC no aporta nada: usa JPEG, WebP o AVIF para fotografías.",
        "verdict_good": "Buena opción: {pct}% más pequeño que {best}.",
        "verdict_bad": "No es buena opción: {best} es más pequeño. Usa ese.",
        "h_bandwidth": "Cómo se calcula el ancho de banda",
        "bandwidth": ("Los archivos de imagen ya están comprimidos, así que el servidor no los reduce más. "
                      "Los bytes ahorrados en cada transferencia son, por tanto, la diferencia de tamaño. "
                      "Multiplícala por el número de vistas para obtener el total. Un ahorro de 20&nbsp;% en "
                      "una captura de 150&nbsp;KiB son unos 30&nbsp;KiB por vista: importa en un sitio con "
                      "mucho tráfico y poco en una sola página."),
        "h_view": "Ver un archivo .bic",
        "view": ("Los navegadores todavía no abren archivos <code>.bic</code>. Descarga el programa "
                 "<a href=\"https://github.com/proteo5/bic/releases\">bic-view</a> para Windows, Linux o "
                 "macOS, o convierte el archivo a PNG con <code>bic decode archivo.bic archivo.png</code>. "
                 "Una extensión del navegador o un Service Worker que muestre <code>.bic</code> en "
                 "<code>&lt;img&gt;</code> es el siguiente paso posible."),
        "h_method": "Cómo se obtienen los números",
        "method": [
            "BIC: <code>bic encode</code> con la configuración predeterminada (sin pérdida, Zstandard "
            "nivel 19). Cada archivo se verifica con <code>bic verify</code>, que exige que los píxeles "
            "decodificados coincidan exactamente con el original.",
            "PNG: el codificador predeterminado del crate <code>image</code>. oxipng: preset 2. WebP sin "
            "pérdida: el codificador del crate <code>image</code>.",
            "Formatos con pérdida: Pillow {pillow}. WebP calidad 80, método 6. AVIF calidad 50. JPEG "
            "calidad 85, con la transparencia aplanada sobre blanco.",
            "Las fotografías se guardan como fuentes JPEG (calidad 90). Su archivo BIC conserva ese JPEG.",
            "BIC guarda un perfil de color ICC cuando la fuente lo tiene. En imágenes pequeñas esto suma "
            "unos cientos de bytes.",
            "Las imágenes fuente son los archivos listados abajo, reducidos a 1600 píxeles por lado como "
            "máximo cuando eran más grandes. Los mismos píxeles se usan en todos los formatos.",
        ],
        "h_sources": "Fuentes y licencias",
        "by": "por",
        "unknown": "desconocido",
        "resized": "Reducida para caber en 1600 px.",
        "footer": ("Generado por <code>site/build.py</code> desde el "
                   "<a href=\"https://github.com/proteo5/bic\">repositorio de BIC</a>. Las imágenes se "
                   "republican bajo sus licencias originales, con los cambios indicados arriba."),
    },
}


def verdict(row, s):
    if row["kept_original"]:
        return s["verdict_photo"]
    if row["saving_vs_best_lossless"] > 0:
        return s["verdict_good"].format(pct=f"{row['saving_vs_best_lossless']:.0f}",
                                        best=row["best_lossless"])
    return s["verdict_bad"].format(best=row["best_lossless"])


def description_for(row, lang):
    return row["description"] if lang == "en" else DESCRIPTIONS_ES[row["id"]]


def render_table(rows, s, lang):
    head = "".join(f"<th>{h}</th>" for h in s["th"])
    up = "" if lang == "en" else "../"  # Spanish page lives one level down
    body = []
    for r in rows:
        file_name = html.escape(r["file"])
        stem = html.escape(os.path.splitext(r["file"])[0])
        lossy_note = ""
        if r["kept_original"]:
            lossy_note = f"<br><small>{s['lossy_photo_note']}</small>"
        elif r["lossy_smaller_than_bic"]:
            note = s["lossy_note"].format(fmt=html.escape(r["best_lossy"]),
                                          pct=f"{r['lossy_saving']:.0f}")
            lossy_note = f"<br><small>{note}</small>"
        saving = (s["na"] if r["saving_vs_best_lossless"] is None
                  else f"{r['saving_vs_best_lossless']:+.0f}%")
        per_views = (s["na"] if r["mb_per_1000_views"] is None
                     else f"{r['mb_per_1000_views']:+.2f}")
        body.append(
            "<tr>"
            f"<td><a href='{up}img/{file_name}'><img class='thumb' src='{up}img/{file_name}' alt=''></a>"
            f"{html.escape(description_for(r, lang))}<br><small>{s['kind'][r['kind']]}</small></td>"
            f"<td>{r['width']}×{r['height']}</td>"
            f"<td>{kib(r['lossless']['PNG'])}</td>"
            f"<td>{kib(r['lossless']['oxipng'])}</td>"
            f"<td>{kib(r['lossless']['WebP lossless'])}</td>"
            f"<td><strong>{kib(r['bic_bytes'])}</strong>"
            f"<br><small><a href='{up}bic/{stem}.bic'>.bic</a></small></td>"
            f"<td>{html.escape(r['best_lossless'])}: {kib(r['lossless'][r['best_lossless']])}</td>"
            f"<td>{saving}</td>"
            f"<td>{per_views}</td>"
            f"<td>{kib(r['lossy']['WebP q80'])}</td>"
            f"<td>{kib(r['lossy']['AVIF q50'])}</td>"
            f"<td>{kib(r['lossy']['JPEG q85'])}</td>"
            f"<td>{verdict(r, s)}{lossy_note}</td>"
            "</tr>"
        )
    return f"<table><thead><tr>{head}</tr></thead><tbody>{''.join(body)}</tbody></table>"


def render_attribution(rows, s, lang):
    items = []
    for r in rows:
        license_link = (f"<a href='{html.escape(r['license_url'])}'>{html.escape(r['license'])}</a>"
                        if r["license_url"] else html.escape(r["license"]))
        resized = " " + s["resized"] if r["resized"] else ""
        items.append(
            f"<li>{html.escape(description_for(r, lang))}: "
            f"<a href='{html.escape(r['page_url'])}'>{html.escape(r['source_file'])}</a>, "
            f"{s['by']} {html.escape(r['author'] or s['unknown'])}, {license_link}.{resized}</li>"
        )
    return "<ul>" + "".join(items) + "</ul>"


PAGE = """<!doctype html>
<html lang="{lang_code}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  :root {{ color-scheme: light dark; --muted: #6b7280; --line: #d1d5db; }}
  body {{ font: 16px/1.5 system-ui, sans-serif; max-width: 1200px; margin: 2rem auto; padding: 0 1rem; }}
  nav {{ text-align: right; }}
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
<nav><a href="{switch_href}" hreflang="{switch_other}">{switch_label}</a></nav>
<h1>{h1}</h1>
<p class="lead">{lead}</p>

<div class="note">
<strong>{note_win}</strong> {note_win_text}<br>
<strong>{note_lose}</strong> {note_lose_text}<br>
<strong>{note_photo}</strong> {note_photo_text}
</div>

<h2>{h_results}</h2>
<p>{results_intro}</p>
<div class="wrap">{table}</div>

<h2>{h_bandwidth}</h2>
<p>{bandwidth}</p>

<h2>{h_view}</h2>
<p>{view}</p>

<h2>{h_method}</h2>
<ul>
{method}
</ul>

<h2>{h_sources}</h2>
{attribution}

<p><small>{footer}</small></p>
</body>
</html>
"""


def render_page(rows, lang, pillow_version):
    s = STRINGS[lang]
    other = "es" if lang == "en" else "en"
    return PAGE.format(
        lang_code=s["lang_code"],
        title=s["title"],
        switch_href=s["switch_href"],
        switch_label=s["switch_label"],
        switch_other=other,
        h1=s["h1"],
        lead=s["lead"],
        note_win=s["note_win"],
        note_win_text=s["note_win_text"],
        note_lose=s["note_lose"],
        note_lose_text=s["note_lose_text"],
        note_photo=s["note_photo"],
        note_photo_text=s["note_photo_text"],
        h_results=s["h_results"],
        results_intro=s["results_intro"],
        table=render_table(rows, s, lang),
        h_bandwidth=s["h_bandwidth"],
        bandwidth=s["bandwidth"],
        h_view=s["h_view"],
        view=s["view"],
        h_method=s["h_method"],
        method="\n".join(f"<li>{m.format(pillow=pillow_version)}</li>" for m in s["method"]),
        h_sources=s["h_sources"],
        attribution=render_attribution(rows, s, lang),
        footer=s["footer"],
    )


def main():
    import PIL

    manifest_path = os.path.join(SOURCES, "manifest.json")
    if not os.path.exists(manifest_path):
        sys.exit("run site/prepare_sources.py first")
    if not os.path.exists(BIC):
        sys.exit(f"bic binary not found at {BIC}; run cargo build --release")
    with open(manifest_path, encoding="utf-8") as f:
        manifest = json.load(f)
    rows = build_rows(manifest)

    os.makedirs(os.path.join(OUT, "es"), exist_ok=True)
    with open(os.path.join(OUT, "index.html"), "w", encoding="utf-8") as f:
        f.write(render_page(rows, "en", PIL.__version__))
    with open(os.path.join(OUT, "es", "index.html"), "w", encoding="utf-8") as f:
        f.write(render_page(rows, "es", PIL.__version__))
    with open(os.path.join(OUT, "results.json"), "w", encoding="utf-8") as f:
        json.dump(rows, f, ensure_ascii=False, indent=1)
    print(f"wrote {os.path.join(OUT, 'index.html')} and {os.path.join(OUT, 'es', 'index.html')}")


if __name__ == "__main__":
    main()
