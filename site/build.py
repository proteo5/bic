"""Build the BIC site: the demo page and the documentation, in English and Spanish.

Measures every example (sizes of the lossless and lossy alternatives, and the
.bic file the encoder writes), verifies each .bic with `bic verify`, and writes
a static site to site/out:

  index.html, es/index.html            the demo page
  docs/index.html, es/docs/index.html  the documentation
  assets/style.css, assets/app.js      styles (light and dark) and the page script
  bic/, img/                           the example files
  results.json, size.json              the measurements

Every number on the site comes from this build. The live examples are decoded
in the visitor's browser by the published @proteo5/bic-runtime package.

Usage (after `cargo build --release --bin bic` and `npm ci` in site/):
    python site/build.py
"""

import csv
import hashlib
import html
import io
import json
import os
import shutil
import posixpath
import subprocess
import sys

from PIL import Image

from docs_text import DOCS

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SOURCES = os.path.join(HERE, "sources")
OUT = os.path.join(HERE, "out")
APP = os.path.join(HERE, "app")
BIC = os.environ.get(
    "BIC_BIN",
    os.path.join(ROOT, "target", "release", "bic" + (".exe" if os.name == "nt" else "")),
)
RUNTIME_PACKAGE = os.path.join(HERE, "node_modules", "@proteo5", "bic-runtime", "package.json")
FIXTURES = os.path.join(ROOT, "packages", "bic-runtime", "test", "fixtures", "manifest.json")
MB = 1_000_000  # bandwidth is reported in decimal megabytes
REPO = "https://github.com/proteo5/bic"


def run(*args, cwd=None):
    result = subprocess.run(args, capture_output=True, text=True, check=False, cwd=cwd)
    if result.returncode != 0:
        sys.exit(f"command failed: {' '.join(args)}\n{result.stderr or result.stdout}")
    return result.stdout


# ---- Measurements ---------------------------------------------------------


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
    rgba = image.convert("RGBA")
    flat = Image.new("RGB", rgba.size, (255, 255, 255))
    flat.paste(rgba, mask=rgba.getchannel("A"))
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
    """Pixel-level sizes from `bic compare`: PNG, oxipng, WebP lossless, BIC's own coding."""
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


def sha256_of_pixels(path):
    return hashlib.sha256(Image.open(path).convert("RGBA").tobytes()).hexdigest()


def sha256_of_file(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def kib(n):
    return f"{n / 1024:,.1f}"


def pct(a, b):
    """How much smaller `a` is than `b`, in percent. Negative when `a` is larger."""
    return (1 - a / b) * 100


DESCRIPTIONS_ES = {
    "ui-screenshot": "Interfaz de software (captura)",
    "flowchart": "Diagrama de flujo",
    "bar-chart": "Gráfica de barras",
    "icon": "Ícono con transparencia",
    "pixel-art": "Pixel art",
    "photo-landscape-1": "Fotografía de paisaje",
    "photo-landscape-2": "Fotografía de paisaje",
}


def build_rows(manifest):
    os.makedirs(os.path.join(OUT, "bic"), exist_ok=True)
    os.makedirs(os.path.join(OUT, "img"), exist_ok=True)

    bench = os.path.join(OUT, "bench.csv")
    run(BIC, "compare", SOURCES, "--csv", bench)
    native = native_measurements(bench)

    rows = []
    for entry in manifest:
        source = os.path.join(SOURCES, entry["file"])
        stem = os.path.splitext(entry["file"])[0]
        bic_path = os.path.join(OUT, "bic", stem + ".bic")
        shutil.copy(source, os.path.join(OUT, "img", entry["file"]))
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

        if bic["kept_original"]:
            # The source is lossy already: a lossless comparison does not apply.
            saving = None
            mb_per_1000 = None
            expected = sha256_of_file(source)
            kind = "file"
        else:
            saving = pct(bic["bytes"], best_lossless)
            mb_per_1000 = (best_lossless - bic["bytes"]) * 1000 / MB
            expected = sha256_of_pixels(source)
            kind = "pixels"
        with Image.open(source) as image:
            width, height = image.size
        rows.append({
            **entry,
            "stem": stem,
            "width": width,
            "height": height,
            "bic_bytes": bic["bytes"],
            "kept_original": bic["kept_original"],
            "kind": kind,
            "sha256": expected,
            "lossless": lossless,
            "lossy": lossy_named,
            "best_lossless": best_lossless_name,
            "best_lossy": best_lossy_name,
            "saving_vs_best_lossless": saving,
            "mb_per_1000_views": mb_per_1000,
            "lossy_smaller_than_bic": best_lossy < bic["bytes"],
            "lossy_saving": pct(best_lossy, bic["bytes"]),
            "decode_ms": pixels["decode_ms"],
        })
        print(f"{entry['id']:20} bic {bic['bytes']:>9} best lossless {best_lossless_name} "
              f"{best_lossless:>9} saving {'n/a' if saving is None else f'{saving:+6.1f}%'}")
    return rows


def example_title(row, lang):
    return row["description"] if lang == "en" else DESCRIPTIONS_ES.get(row["id"], row["description"])


# ---- Page text ------------------------------------------------------------

HOME = {
    "en": {
        "lang": "en",
        "title": "BIC: a lossless image format for charts, logos and icons",
        "description": "BIC is a lossless image format: smaller than optimized PNG on some flat graphics; photographs stay JPEG. Decoded in the browser by a 5.7 KiB library.",
        "nav_home": "Home", "nav_docs": "Documentation", "nav_results": "Results", "nav_live": "Live examples",
        "lang_switch": "Español", "lang_href": "../es/", "lang_hreflang": "es",
        "theme_dark": "Switch to dark theme", "theme_light": "Switch to light theme",
        "eyebrow": "Lossless · open source · Apache-2.0",
        "h1": "A lossless image format for charts, logos and icons",
        "lead": "BIC stores flat graphics in fewer bytes than optimized PNG. Photographs stay as their original JPEG. The decoder runs in the browser, and you can try it below on real files.",
        "cta_docs": "Read the documentation", "cta_github": "View on GitHub", "cta_npm": "npm package",
        "install_runtime": "Decoder for browsers and Node.js", "install_build": "Build step for Vite and webpack",
        "copy": "Copy", "copied": "Copied",
        "stats": [("{decoder} KiB", "decoder, gzip, one dependency (fzstd)"),
                  ("{fixtures}", "test cases checked against the Rust decoder"),
                  ("{best}%", "smaller than the best lossless format, best example")],
        "why_title": "Why BIC: save bandwidth on solid-color images",
        "why_lead": "Charts, interface screenshots, logos, icons and diagrams are made of large areas of one color. A PNG already compresses them, but BIC stores each run of identical pixels in a few bytes and compresses the result again. A smaller file is less data to download on every visit, and on a slow connection it is the difference between an image that appears at once and one that waits.",
        "why_points": [
            "Good fit: flat graphics with few colors. For them BIC can be smaller than optimized PNG (see the results).",
            "Not a fit: photographs have no long runs of equal pixels. BIC keeps their original JPEG inside the file and adds 32 bytes, so use JPEG or WebP for them.",
            "Exact: each .bic decodes to the same pixels as its source. The examples below check that byte by byte.",
        ],
        "export_title": "Export your own image to .bic",
        "export_lead": "This page decodes .bic files but does not encode them yet, so exporting uses the command line. These commands compare your file the same way as the results table.",
        "export_cmd": "bic inspect photo.png --json   # sizes of PNG, oxipng, WebP and BIC\nbic encode photo.png photo.bic   # keeps the smaller strategy\nbic verify photo.png photo.bic   # decoded pixels must match exactly",
        "export_strategy": [
            "The BIC algorithm: when it is smaller than the source. This is the usual result for charts, interfaces, icons and diagrams.",
            "A wrapper: when the source file is already smaller (PNG, JPEG, WebP or HEIF), it is kept inside the .bic unchanged. Add --transcode to store the BIC algorithm anyway.",
        ],
        "live_title": "Live: real .bic files, decoded in your browser",
        "live_lead": "Each file below is decoded right now by @proteo5/bic-runtime from npm, with no server involved. The result is compared with the source pixels, byte for byte.",
        "drop_title": "Try your own .bic file",
        "drop_text": "Drop a .bic file here, or choose one. It is decoded in your browser; nothing is uploaded.",
        "drop_choose": "Choose file", "drop_png": "Download as PNG",
        "example_png": "PNG source", "example_bic": ".bic file",
        "results_title": "Results",
        "results_lead": "Measured on the example images below. Sizes in KiB. Each row used one of two strategies: the BIC algorithm (flat graphics), or a wrapper that keeps the original JPEG inside the .bic with 32 bytes of header (photographs). \"Best lossless\" is the smallest of PNG, oxipng and WebP lossless. The last three columns are lossy formats, which change pixel values. \"MB saved per 1,000 views\" multiplies the difference by 1,000 transfers. Negative values mean BIC is larger.",
        "th": ["Example", "Pixels", "PNG", "oxipng", "WebP lossless", "BIC file", "Best lossless", "BIC smaller by", "MB saved / 1,000 views", "WebP q80", "AVIF q50", "JPEG q85", "Verdict"],
        "pill_good": "Use BIC", "pill_bad": "Keep the PNG", "pill_jpeg": "Keep the JPEG",
        "verdict_good": "{pct}% smaller than {best}.", "verdict_bad": "{best} is smaller. Keep it.",
        "verdict_photo": "The original JPEG is stored as it is, so BIC adds nothing here.",
        "lossy_note": "Lossy {fmt} is {pct}% smaller than BIC, if exact pixels are not needed.",
        "lossy_photo_note": "Lossless formats are far larger for photographs.",
        "na": "n/a",
        "use_title": "When to use BIC",
        "use_lead": "The measurements decide the cases. Flat graphics with few colors are where BIC is smaller than optimized PNG. Everything else in the table is not.",
        "use_do_title": "Use BIC for", "use_do": ["Charts, diagrams and graphs with solid areas", "Logos and brand marks", "Icons and pixel art with few colors", "Any image that must stay lossless and is smaller in BIC"],
        "use_dont_title": "Do not use BIC for", "use_dont": ["Photographs: store them as JPEG or WebP", "Software screenshots with anti-aliased text: they have thousands of colors and BIC is larger", "Images that may be lossy: WebP or AVIF are much smaller"],
        "use_rule": "The build step converts an image only when BIC is at least 10% smaller than the best lossless format, and never for JPEG or for images with more than 4096 colors.",
        "integrate_title": "Use it in your project",
        "integrate_lead": "The decoder works wherever JavaScript runs. The build step converts your images and serves them through a Service Worker, under their original URLs, so your HTML and CSS do not change.",
        "more_docs": "Full reference in the documentation →",
        "how_title": "How it works",
        "how": ["Images are encoded in planes, with palettes or color transforms, predictors and run-length blocks, then compressed with Zstandard.",
                "The encoder tries a small set of candidates and keeps the smallest. The result decodes to exactly the same pixels.",
                "With the build step, a Service Worker answers requests for converted images. Without it, the browser downloads the PNG as usual.",
                "The decoded image is kept in the browser's cache, so each .bic is downloaded once per device."],
        "verify_title": "How it is checked",
        "verify": ["53 cases made by the Rust encoder: each decodes in the JavaScript runtime to the Rust decoder's exact pixels.",
                   "Malformed and randomly corrupted files throw an error, never a crash.",
                   "End-to-end tests in Chrome with Vite and webpack: the first visit downloads the PNG; later visits show the .bic with identical pixels.",
                   "Not tested yet: Firefox and Safari."],
        "downloads_title": "Downloads",
        "downloads_lead": "The command-line tool and the viewer for Windows, Linux and macOS, from release v{version}. The build step installs the right binary by itself.",
        "download_items": [("Windows x64", "bic-v{version}-x86_64-pc-windows-msvc.zip"), ("Linux x64", "bic-v{version}-x86_64-unknown-linux-gnu.tar.gz"),
                           ("macOS Apple Silicon", "bic-v{version}-aarch64-apple-darwin.tar.gz"), ("macOS Intel", "bic-v{version}-x86_64-apple-darwin.tar.gz")],
        "download_link": "Download", "download_all": "All releases and checksums",
        "sources_title": "Example images and licenses",
        "sources_lead": "Each example is republished under its license, from Wikimedia Commons. Images larger than 1600 px were resized, as noted.",
        "by": "by", "unknown": "unknown", "resized": "Resized to fit 1600 px.",
        "footer": "Built from the BIC repository. Decoder used on this site: @proteo5/bic-runtime {runtime}.",
        "footer_source": "Source", "footer_license": "Apache-2.0",
        "ui": {"themeDark": "Switch to dark theme", "themeLight": "Switch to light theme", "decodedIn": "BIC algorithm, decoded in {ms} ms",
               "storedAs": "Wrapper: the original {mime} is kept inside the .bic, not re-encoded", "identical": "identical to the source",
               "mismatch": "does not match the source", "failed": "Could not load", "notBic": "Not a valid .bic file",
               "dropDone": "Decoded. The decoder checks the pixels itself.", "downloadPng": "Download as PNG"},
    },
    "es": {
        "lang": "es",
        "title": "BIC: un formato de imagen sin pérdida para gráficas, logotipos e íconos",
        "description": "BIC es un formato de imagen sin pérdida: ocupa menos que un PNG optimizado en algunos gráficos planos; las fotografías se quedan en JPEG. Se decodifica en el navegador con una librería de 5.7 KiB.",
        "nav_home": "Inicio", "nav_docs": "Documentación", "nav_results": "Resultados", "nav_live": "Ejemplos en vivo",
        "lang_switch": "English", "lang_href": "../", "lang_hreflang": "en",
        "theme_dark": "Cambiar al tema oscuro", "theme_light": "Cambiar al tema claro",
        "eyebrow": "Sin pérdida · código abierto · Apache-2.0",
        "h1": "Un formato de imagen sin pérdida para gráficas, logotipos e íconos",
        "lead": "BIC guarda los gráficos planos en menos bytes que un PNG optimizado. Las fotografías se conservan como su JPEG original. El decodificador corre en el navegador, y puedes probarlo abajo con archivos reales.",
        "cta_docs": "Leer la documentación", "cta_github": "Ver en GitHub", "cta_npm": "Paquete en npm",
        "install_runtime": "Decodificador para navegador y Node.js", "install_build": "Paso de build para Vite y webpack",
        "copy": "Copiar", "copied": "Copiado",
        "stats": [("{decoder} KiB", "decodificador, gzip, una sola dependencia (fzstd)"),
                  ("{fixtures}", "casos verificados contra el decodificador de Rust"),
                  ("{best}%", "más pequeño que el mejor formato sin pérdida, mejor caso")],
        "why_title": "Por qué BIC: ahorrar ancho de banda en imágenes de color sólido",
        "why_lead": "Las gráficas, capturas de interfaz, logotipos, iconos y diagramas están hechos de grandes áreas de un solo color. Un PNG ya los comprime, pero BIC guarda cada tramo de píxeles idénticos en pocos bytes y después vuelve a comprimir el resultado. Un archivo más pequeño son menos datos que descargar en cada visita, y en una conexión lenta es la diferencia entre una imagen que aparece de inmediato y una que tarda.",
        "why_points": [
            "Buen caso: gráficos planos con pocos colores. Para ellos BIC puede ser más pequeño que un PNG optimizado (ver los resultados).",
            "No es buen caso: las fotografías no tienen tramos largos de píxeles iguales. BIC conserva su JPEG original dentro del archivo y añade 32 bytes, así que para fotos usa JPEG o WebP.",
            "Exacto: cada .bic se decodifica a los mismos píxeles que su original. Los ejemplos de abajo lo comprueban byte por byte.",
        ],
        "export_title": "Exporta tu propia imagen a .bic",
        "export_lead": "Esta página decodifica archivos .bic, pero todavía no los codifica, así que exportar se hace desde la línea de comandos. Estos comandos comparan tu archivo igual que la tabla de resultados.",
        "export_cmd": "bic inspect foto.png --json   # tamaños de PNG, oxipng, WebP y BIC\nbic encode foto.png foto.bic   # conserva la estrategia más pequeña\nbic verify foto.png foto.bic   # los píxeles decodificados deben coincidir exactamente",
        "export_strategy": [
            "El algoritmo BIC: cuando es más pequeño que el original. Es el resultado habitual en gráficas, interfaces, iconos y diagramas.",
            "Un envoltorio: cuando el archivo original ya es más pequeño (PNG, JPEG, WebP o HEIF), se guarda dentro del .bic sin cambios. Añade --transcode para guardar el algoritmo BIC de todas formas.",
        ],
        "live_title": "En vivo: archivos .bic reales, decodificados en tu navegador",
        "live_lead": "Cada archivo de abajo lo decodifica ahora @proteo5/bic-runtime desde npm, sin servidor de por medio. El resultado se compara con los píxeles originales, byte por byte.",
        "drop_title": "Prueba tu propio archivo .bic",
        "drop_text": "Arrastra un archivo .bic aquí, o elígelo. Se decodifica en tu navegador; no se sube nada.",
        "drop_choose": "Elegir archivo", "drop_png": "Descargar como PNG",
        "example_png": "PNG original", "example_bic": "archivo .bic",
        "results_title": "Resultados",
        "results_lead": "Medido en las imágenes de ejemplo. Tamaños en KiB. Cada fila usó una de dos estrategias: el algoritmo BIC (gráficos planos), o un envoltorio que guarda el JPEG original dentro del .bic con 32 bytes de cabecera (fotografías). \"Mejor sin pérdida\" es el más pequeño de PNG, oxipng y WebP sin pérdida. Las últimas tres columnas son formatos con pérdida, que cambian los valores de los píxeles. \"MB ahorrados por cada 1,000 vistas\" multiplica la diferencia por 1,000 transferencias. Los valores negativos significan que BIC es más grande.",
        "th": ["Ejemplo", "Píxeles", "PNG", "oxipng", "WebP sin pérdida", "Archivo BIC", "Mejor sin pérdida", "BIC más pequeño por", "MB ahorrados / 1,000 vistas", "WebP q80", "AVIF q50", "JPEG q85", "Veredicto"],
        "pill_good": "Usa BIC", "pill_bad": "Mantén el PNG", "pill_jpeg": "Mantén el JPEG",
        "verdict_good": "{pct}% más pequeño que {best}.", "verdict_bad": "{best} es más pequeño. Mantenlo.",
        "verdict_photo": "El JPEG original se guarda tal cual, así que BIC no aporta nada aquí.",
        "lossy_note": "{fmt} con pérdida es {pct}% más pequeño que BIC, si no se necesitan píxeles exactos.",
        "lossy_photo_note": "Los formatos sin pérdida son mucho más grandes para fotografías.",
        "na": "n/d",
        "use_title": "Cuándo usar BIC",
        "use_lead": "Las mediciones deciden los casos. Los gráficos planos con pocos colores son donde BIC ocupa menos que un PNG optimizado. Lo demás de la tabla no.",
        "use_do_title": "Usa BIC para", "use_do": ["Gráficas, diagramas y esquemas con áreas sólidas", "Logotipos y marcas", "Íconos y pixel art con pocos colores", "Cualquier imagen que deba seguir sin pérdida y sea más pequeña en BIC"],
        "use_dont_title": "No uses BIC para", "use_dont": ["Fotografías: guárdalas como JPEG o WebP", "Capturas de software con texto suavizado: tienen miles de colores y BIC es más grande", "Imágenes que pueden ser con pérdida: WebP o AVIF son mucho más pequeños"],
        "use_rule": "El paso de build convierte una imagen solo cuando BIC es al menos 10% más pequeño que el mejor formato sin pérdida, y nunca para JPEG ni para imágenes con más de 4096 colores.",
        "integrate_title": "Úsalo en tu proyecto",
        "integrate_lead": "El decodificador funciona donde corre JavaScript. El paso de build convierte tus imágenes y las sirve con un Service Worker, bajo sus URL originales, así que tu HTML y tu CSS no cambian.",
        "more_docs": "Referencia completa en la documentación →",
        "how_title": "Cómo funciona",
        "how": ["Las imágenes se codifican por planos, con paletas o transformadas de color, predictores y bloques de longitud de series, y luego se comprimen con Zstandard.",
                "El codificador prueba un conjunto pequeño de candidatos y se queda con el más pequeño. El resultado se decodifica a exactamente los mismos píxeles.",
                "Con el paso de build, un Service Worker responde las peticiones de las imágenes convertidas. Sin él, el navegador descarga el PNG de siempre.",
                "La imagen decodificada queda en la caché del navegador, así que cada .bic se descarga una vez por dispositivo."],
        "verify_title": "Cómo se verifica",
        "verify": ["53 casos hechos con el codificador de Rust: cada uno se decodifica en el runtime de JavaScript a los píxeles exactos del decodificador de Rust.",
                   "Los archivos mal formados o dañados al azar producen un error, nunca un fallo.",
                   "Pruebas de extremo a extremo en Chrome con Vite y webpack: la primera visita descarga el PNG; las siguientes muestran el .bic con píxeles idénticos.",
                   "Sin probar todavía: Firefox y Safari."],
        "downloads_title": "Descargas",
        "downloads_lead": "La herramienta de línea de comandos y el visor para Windows, Linux y macOS, del release v{version}. El paso de build instala el binario correcto por sí solo.",
        "download_items": [("Windows x64", "bic-v{version}-x86_64-pc-windows-msvc.zip"), ("Linux x64", "bic-v{version}-x86_64-unknown-linux-gnu.tar.gz"),
                           ("macOS Apple Silicon", "bic-v{version}-aarch64-apple-darwin.tar.gz"), ("macOS Intel", "bic-v{version}-x86_64-apple-darwin.tar.gz")],
        "download_link": "Descargar", "download_all": "Todos los releases y sumas de verificación",
        "sources_title": "Imágenes de ejemplo y licencias",
        "sources_lead": "Cada ejemplo se republica bajo su licencia, desde Wikimedia Commons. Las imágenes de más de 1600 px se redujeron, como se indica.",
        "by": "por", "unknown": "desconocido", "resized": "Reducida para caber en 1600 px.",
        "footer": "Sitio construido desde el repositorio de BIC. Decodificador usado en este sitio: @proteo5/bic-runtime {runtime}.",
        "footer_source": "Código fuente", "footer_license": "Apache-2.0",
        "ui": {"themeDark": "Cambiar al tema oscuro", "themeLight": "Cambiar al tema claro", "decodedIn": "Algoritmo BIC, decodificado en {ms} ms",
               "storedAs": "Envoltorio: el {mime} original va dentro del .bic, sin recodificar", "identical": "idéntico al original",
               "mismatch": "no coincide con el original", "failed": "No se pudo cargar", "notBic": "No es un archivo .bic válido",
               "dropDone": "Decodificado. El decodificador verifica los píxeles por sí mismo.", "downloadPng": "Descargar como PNG"},
    },
}


# ---- Page layout ----------------------------------------------------------


def root_of(lang):
    """Folder of a language's site, relative to the site root."""
    return "" if lang == "en" else "es/"


def page_dir(page):
    """Folder of a page inside its language, relative to the language root."""
    return "" if page == "home" else "docs/"


def prefix_for(lang, page):
    """Relative path from a page back to the site root ('', '../', '../../'...)."""
    here = root_of(lang) + page_dir(page)
    return posixpath.relpath(".", here or ".") + "/" if here else ""


def href(lang, page, from_lang, from_page):
    """Relative link from one page to another page (in any language)."""
    here = root_of(from_lang) + page_dir(from_page)
    there = root_of(lang) + page_dir(page)
    rel = posixpath.relpath(there or ".", here or ".")
    return rel if rel.endswith("/") else rel + "/"


def page_shell(lang, page, title, description, body, data_json, runtime_version):
    """Common frame: header with navigation, theme and language switch, footer, scripts."""
    t = HOME[lang]
    other = "es" if lang == "en" else "en"
    prefix = prefix_for(lang, page)
    nav_items = [(href(lang, "home", lang, page), t["nav_home"], "home")]
    if page == "home":
        nav_items += [("#live", t["nav_live"], ""), ("#results", t["nav_results"], "")]
    nav_items.append((href(lang, "docs", lang, page), t["nav_docs"], "docs"))
    nav = "".join(
        f'<a href="{link}"' + (' aria-current="page"' if key == page else "") + f">{label}</a>"
        for link, label, key in nav_items
    )
    switch = href(other, page, lang, page)
    home_link = href(lang, "home", lang, page)
    return f"""<!doctype html>
<html lang="{t['lang']}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(title)}</title>
<meta name="description" content="{html.escape(description)}">
<meta property="og:title" content="{html.escape(title)}">
<meta property="og:description" content="{html.escape(description)}">
<meta name="color-scheme" content="light dark">
<script>
(function () {{
  try {{
    var saved = localStorage.getItem('bic-theme');
    if (saved === 'dark' || saved === 'light') document.documentElement.dataset.theme = saved;
  }} catch (e) {{}}
}})();
</script>
<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='8' fill='%232563eb'/%3E%3Ctext x='16' y='22' font-size='17' text-anchor='middle' fill='white' font-family='Arial' font-weight='700'%3EB%3C/text%3E%3C/svg%3E">
<link rel="stylesheet" href="{prefix}assets/style.css">
<link rel="alternate" hreflang="{HOME[other]['lang']}" href="{switch}">
</head>
<body>
<header class="site-header">
  <div class="container">
    <a class="brand" href="{home_link}"><span class="brand-mark">B</span>BIC</a>
    <nav class="nav" aria-label="Main">{nav}</nav>
    <div class="header-actions">
      <a class="lang-switch" href="{switch}" hreflang="{HOME[other]['lang']}">{t['lang_switch']}</a>
      <button id="theme-toggle" class="icon-button" type="button" aria-label="{t['theme_dark']}" title="{t['theme_dark']}">
        <svg class="theme-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
          <path class="moon" d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/>
          <g class="sun"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></g>
        </svg>
      </button>
    </div>
  </div>
</header>
<main>
{body}
</main>
<footer>
  <div class="container">
    <span>{t['footer'].format(runtime=runtime_version)}</span>
    <span><a href="{REPO}">{t['footer_source']}</a> · {t['footer_license']}</span>
  </div>
</footer>
<script type="application/json" id="bic-data">{data_json}</script>
<script src="{prefix}assets/app.js" defer></script>
</body>
</html>
"""


def page_data(rows, prefix, t, runtime_version):
    """The JSON the page script reads: translated UI strings and the examples."""
    payload = {
        "runtimeVersion": runtime_version,
        "ui": t["ui"],
        "examples": [
            {"id": r["stem"], "bic": f"{prefix}bic/{r['stem']}.bic", "sha256": r["sha256"], "kind": r["kind"]}
            for r in rows
        ],
    }
    return json.dumps(payload, ensure_ascii=False).replace("<", "\\u003c")


def render_home(rows, lang, runtime_version, stats):
    t = HOME[lang]
    prefix = prefix_for(lang, "home")
    stat_html = "".join(
        f'<div class="stat"><strong>{value}</strong><span>{label}</span></div>' for value, label in stats
    )
    example_cards = []
    for r in rows:
        stem = html.escape(r["stem"])
        file_name = html.escape(r["file"])
        example_cards.append(f"""
        <article class="card example" data-example="{stem}">
          <div class="example-head">
            <h3>{html.escape(example_title(r, lang))}</h3>
            <small>{r['width']} × {r['height']}</small>
          </div>
          <div class="view" aria-live="polite"><span class="meta">…</span></div>
          <p class="status" data-state="">&nbsp;</p>
          <div class="example-links">
            <a href="{prefix}img/{file_name}">{t['example_png']}</a>
            <a href="{prefix}bic/{stem}.bic">{t['example_bic']}</a>
          </div>
        </article>""")

    head = "".join(f"<th>{h}</th>" for h in t["th"])
    body_rows = []
    for r in rows:
        saving = t["na"] if r["saving_vs_best_lossless"] is None else f"{r['saving_vs_best_lossless']:+.0f}%"
        per_views = t["na"] if r["mb_per_1000_views"] is None else f"{r['mb_per_1000_views']:+.2f}"
        if r["kept_original"]:
            verdict = f'<span class="pill neutral">{t["pill_jpeg"]}</span> {t["verdict_photo"]}<br><small>{t["lossy_photo_note"]}</small>'
        elif r["saving_vs_best_lossless"] > 0:
            text = t["verdict_good"].format(pct=f"{r['saving_vs_best_lossless']:.0f}", best=r["best_lossless"])
            verdict = f'<span class="pill good">{t["pill_good"]}</span> {html.escape(text)}'
        else:
            text = t["verdict_bad"].format(best=r["best_lossless"])
            verdict = f'<span class="pill bad">{t["pill_bad"]}</span> {html.escape(text)}'
        if not r["kept_original"] and r["lossy_smaller_than_bic"]:
            note = t["lossy_note"].format(fmt=html.escape(r["best_lossy"]), pct=f"{r['lossy_saving']:.0f}")
            verdict += f"<br><small>{html.escape(note)}</small>"
        body_rows.append(
            "<tr>"
            f'<td><img class="thumb" src="{prefix}img/{html.escape(r["file"])}" alt="" loading="lazy">'
            f'{html.escape(example_title(r, lang))}</td>'
            f"<td>{r['width']}×{r['height']}</td>"
            f"<td>{kib(r['lossless']['PNG'])}</td>"
            f"<td>{kib(r['lossless']['oxipng'])}</td>"
            f"<td>{kib(r['lossless']['WebP lossless'])}</td>"
            f"<td><strong>{kib(r['bic_bytes'])}</strong></td>"
            f"<td>{html.escape(r['best_lossless'])}: {kib(r['lossless'][r['best_lossless']])}</td>"
            f"<td>{saving}</td><td>{per_views}</td>"
            f"<td>{kib(r['lossy']['WebP q80'])}</td>"
            f"<td>{kib(r['lossy']['AVIF q50'])}</td>"
            f"<td>{kib(r['lossy']['JPEG q85'])}</td>"
            f"<td>{verdict}</td>"
            "</tr>"
        )
    table = (f'<div class="table-wrap"><table><thead><tr>{head}</tr></thead>'
             f'<tbody>{"".join(body_rows)}</tbody></table></div>')

    attribution = "".join(
        f"<li>{html.escape(example_title(r, lang))}: "
        f'<a href="{html.escape(r["page_url"])}">{html.escape(r["source_file"])}</a>, '
        f'{t["by"]} {html.escape(r["author"] or t["unknown"])}, '
        + (f'<a href="{html.escape(r["license_url"])}">{html.escape(r["license"])}</a>' if r["license_url"]
           else html.escape(r["license"]))
        + "." + (f" {t['resized']}" if r["resized"] else "") + "</li>"
        for r in rows
    )

    downloads = "".join(
        f'<div class="card download"><strong>{label}</strong><span>{pattern.format(version=VERSION)}</span>'
        f'<a href="{REPO}/releases/download/v{VERSION}/{pattern.format(version=VERSION)}">{t["download_link"]}</a></div>'
        for label, pattern in t["download_items"]
    )

    body = f"""
<section class="hero">
  <div class="container">
    <p class="eyebrow">{t['eyebrow']}</p>
    <h1>{t['h1']}</h1>
    <p class="lead">{t['lead']}</p>
    <div class="cta">
      <a class="button primary" href="{prefix}docs/">{t['cta_docs']}</a>
      <a class="button" href="{REPO}">{t['cta_github']}</a>
      <a class="button" href="https://www.npmjs.com/package/@proteo5/bic-runtime">{t['cta_npm']}</a>
    </div>
    <div class="install">
      <div>
        <small class="meta">{t['install_runtime']}</small>
        <div class="install-row"><code>npm install @proteo5/bic-runtime</code>
          <button class="copy" type="button" data-copy-text="npm install @proteo5/bic-runtime">{t['copy']}</button></div>
      </div>
      <div>
        <small class="meta">{t['install_build']}</small>
        <div class="install-row"><code>npm install --save-dev @proteo5/bic-build</code>
          <button class="copy" type="button" data-copy-text="npm install --save-dev @proteo5/bic-build">{t['copy']}</button></div>
      </div>
    </div>
    <div class="hero-stats">{stat_html}</div>
  </div>
</section>

<section id="why" class="alt">
  <div class="container">
    <h2>{t['why_title']}</h2>
    <p class="section-lead">{t['why_lead']}</p>
    <ul class="check">{''.join(f'<li>{html.escape(x)}</li>' for x in t['why_points'])}</ul>
  </div>
</section>

<section id="live">
  <div class="container">
    <h2>{t['live_title']}</h2>
    <p class="section-lead">{t['live_lead']}</p>
    <div class="grid cols-3">{''.join(example_cards)}</div>
    <h3 style="margin-top:2.5rem">{t['drop_title']}</h3>
    <div class="drop" id="drop">
      <p>{t['drop_text']}</p>
      <label class="button primary">
        <input id="drop-input" type="file" accept=".bic" hidden>
        {t['drop_choose']}
      </label>
    </div>
    <div id="drop-result" class="drop-result card" hidden></div>
    <h3 style="margin-top:2.5rem">{t['export_title']}</h3>
    <p class="section-lead">{t['export_lead']}</p>
    <div class="code"><pre><code>{html.escape(t['export_cmd'])}</code></pre></div>
    <ul class="check">{''.join(f'<li>{html.escape(x)}</li>' for x in t['export_strategy'])}</ul>
  </div>
</section>

<section id="results" class="alt">
  <div class="container">
    <h2>{t['results_title']}</h2>
    <p class="section-lead">{t['results_lead']}</p>
    {table}
  </div>
</section>

<section id="use">
  <div class="container">
    <h2>{t['use_title']}</h2>
    <p class="section-lead">{t['use_lead']}</p>
    <div class="grid cols-2">
      <div class="card two-col-do"><h3>{t['use_do_title']}</h3><ul class="check">{''.join(f'<li>{html.escape(x)}</li>' for x in t['use_do'])}</ul></div>
      <div class="card two-col-dont"><h3>{t['use_dont_title']}</h3><ul class="cross">{''.join(f'<li>{html.escape(x)}</li>' for x in t['use_dont'])}</ul></div>
    </div>
    <div class="callout"><p>{t['use_rule']}</p></div>
  </div>
</section>

<section id="integrate" class="alt">
  <div class="container">
    <h2>{t['integrate_title']}</h2>
    <p class="section-lead">{t['integrate_lead']}</p>
    <div class="grid cols-2">
      <div class="card"><h3>{t['how_title']}</h3><ul class="check">{''.join(f'<li>{html.escape(x)}</li>' for x in t['how'])}</ul></div>
      <div class="card"><h3>{t['verify_title']}</h3><ul class="check">{''.join(f'<li>{html.escape(x)}</li>' for x in t['verify'])}</ul></div>
    </div>
    <p style="margin-top:1.5rem"><a href="{prefix}docs/">{t['more_docs']}</a></p>
  </div>
</section>

<section id="downloads">
  <div class="container">
    <h2>{t['downloads_title']}</h2>
    <p class="section-lead">{t['downloads_lead'].format(version=VERSION)}</p>
    <div class="downloads">{downloads}</div>
    <p style="margin-top:1.25rem"><a href="{REPO}/releases">{t['download_all']}</a></p>
  </div>
</section>

<section>
  <div class="container">
    <h2>{t['sources_title']}</h2>
    <p class="section-lead">{t['sources_lead']}</p>
    <ul class="attribution">{attribution}</ul>
  </div>
</section>
"""
    return page_shell(
        lang, "home", t["title"], t["description"], body,
        page_data(rows, prefix, t, runtime_version), runtime_version,
    )


# ---- Documentation pages --------------------------------------------------


def render_docs(lang, runtime_version, rows):
    docs = DOCS[lang]
    t = HOME[lang]
    prefix = prefix_for(lang, "docs")
    toc = "".join(f'<li><a href="#{section["id"]}">{html.escape(section["title"])}</a></li>' for section in docs["sections"])
    sections = "".join(
        f'<section id="{section["id"]}"><h2>{html.escape(section["title"])}</h2>{section["body"]}</section>'
        for section in docs["sections"]
    )
    body = f"""
<section class="hero docs-hero">
  <div class="container">
    <p class="eyebrow">{docs['eyebrow']}</p>
    <h1>{html.escape(docs['h1'])}</h1>
    <p class="lead">{docs['lead']}</p>
  </div>
</section>
<div class="container docs">
  <aside class="toc card" aria-label="{html.escape(docs['toc_label'])}">
    <strong>{html.escape(docs['toc_label'])}</strong>
    <ol>{toc}</ol>
  </aside>
  <div class="docs-body">
    {sections}
  </div>
</div>
"""
    return page_shell(
        lang, "docs", docs["title"], docs["description"], body,
        page_data(rows, prefix, t, runtime_version), runtime_version,
    )


# ---- Main -----------------------------------------------------------------

VERSION = "0.2.1"


def read_version():
    with open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8") as f:
        return next(line.split('"')[1] for line in f if line.startswith("version"))


def main():
    global VERSION
    manifest_path = os.path.join(SOURCES, "manifest.json")
    if not os.path.exists(manifest_path):
        sys.exit("run site/prepare_sources.py first")
    if not os.path.exists(BIC):
        sys.exit(f"bic binary not found at {BIC}; run cargo build --release --bin bic")
    if not os.path.exists(RUNTIME_PACKAGE):
        sys.exit("run `npm ci` in site/ first")
    VERSION = read_version()
    with open(manifest_path, encoding="utf-8") as f:
        manifest = json.load(f)
    with open(RUNTIME_PACKAGE, encoding="utf-8") as f:
        runtime_version = json.load(f)["version"]
    with open(FIXTURES, encoding="utf-8") as f:
        fixtures = len(json.load(f))

    shutil.rmtree(OUT, ignore_errors=True)
    os.makedirs(os.path.join(OUT, "assets"))
    rows = build_rows(manifest)

    # The page script (with the published runtime) and the decoder's size.
    run("node", os.path.join(APP, "bundle.mjs"), cwd=HERE)
    shutil.copy(os.path.join(APP, "style.css"), os.path.join(OUT, "assets", "style.css"))
    with open(os.path.join(OUT, "size.json"), encoding="utf-8") as f:
        size = json.load(f)
    best = max((r["saving_vs_best_lossless"] for r in rows if r["saving_vs_best_lossless"] is not None), default=0)

    for lang in ("en", "es"):
        t = HOME[lang]
        stats = [
            (t["stats"][0][0].format(decoder=f"{size['decoderGzipBytes'] / 1024:.1f}"), t["stats"][0][1]),
            (t["stats"][1][0].format(fixtures=fixtures), t["stats"][1][1]),
            (t["stats"][2][0].format(best=f"{best:.0f}"), t["stats"][2][1]),
        ]
        home_dir = os.path.join(OUT, root_of(lang))
        docs_dir = os.path.join(home_dir, "docs")
        os.makedirs(docs_dir, exist_ok=True)
        with open(os.path.join(home_dir, "index.html"), "w", encoding="utf-8") as f:
            f.write(render_home(rows, lang, runtime_version, stats))
        with open(os.path.join(docs_dir, "index.html"), "w", encoding="utf-8") as f:
            f.write(render_docs(lang, runtime_version, rows))

    # Images and .bic files are shared by all pages; they live at the site root.
    with open(os.path.join(OUT, "results.json"), "w", encoding="utf-8") as f:
        json.dump(rows, f, ensure_ascii=False, indent=1)
    print(f"wrote the site to {OUT} (bic {VERSION}, @proteo5/bic-runtime {runtime_version})")


if __name__ == "__main__":
    main()
