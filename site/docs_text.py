"""Documentation pages of the BIC site, in English (primary) and Spanish.

Every function, option and command named here exists in the released code:
@proteo5/bic-runtime 0.2.1, @proteo5/bic-build 0.2.1 and the bic binary of
release v0.2.1. The site build copies these texts into docs/ and es/docs/.
"""

import html

REPO = "https://github.com/proteo5/bic"


def code(text):
    return f"<pre><code>{html.escape(text.strip(chr(10)))}</code></pre>"


def ul(items):
    return "<ul>" + "".join(f"<li>{item}</li>" for item in items) + "</ul>"


def p(text):
    return f"<p>{text}</p>"


def table(headers, rows):
    head = "".join(f"<th>{h}</th>" for h in headers)
    body = "".join("<tr>" + "".join(f"<td>{c}</td>" for c in row) + "</tr>" for row in rows)
    return f'<div class="table-wrap"><table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table></div>'


PIXELS_SHAPE = "{ kind: 'pixels', width, height, rgba, icc }"
FILE_SHAPE = "{ kind: 'file', mime, bytes, width, height, icc }"


def c(text):
    """Inline code."""
    return f"<code>{html.escape(text)}</code>"


EN = {
    "title": "BIC documentation",
    "description": "Install, use and integrate BIC: the browser decoder, the Vite and webpack plugins, the command line and the file format.",
    "eyebrow": "Documentation",
    "h1": "BIC documentation",
    "lead": "Everything you need to use BIC: the packages, their API, the build plugins, the command line and the format. Examples are real code from the released packages.",
    "toc_label": "On this page",
    "sections": [
        {
            "id": "overview",
            "title": "Overview",
            "body": p("BIC is a lossless image format. It stores charts, logos, icons, diagrams and pixel art in fewer bytes than optimized PNG, and it keeps photographs as their original JPEG (adding only 32 bytes). Three things make up the project:")
            + ul([
                f"{c('@proteo5/bic-runtime')}: a decoder for browsers and Node.js. Zero dependencies except {c('fzstd')}; 5.7 KiB gzip.",
                f"{c('@proteo5/bic-build')}: a build step for Vite and webpack that converts the images that benefit, and a command {c('bic-build')}.",
                f"{c('bic')}: the command-line tool and the viewer, for Windows, Linux and macOS.",
            ])
            + p(f"The file format is described in <a href='{REPO}/blob/main/FORMAT.md'>FORMAT.md</a>."),
        },
        {
            "id": "install",
            "title": "Install",
            "body": p("Install the packages you need. The decoder is enough to read .bic files; the build step is for projects that convert images.")
            + code("npm install @proteo5/bic-runtime\nnpm install --save-dev @proteo5/bic-build")
            + p("The build step needs the <code>bic</code> program. On install it downloads the binary for your platform from the release with the same version number, checks it against the release checksums, and keeps it in <code>vendor/</code>.")
            + table(["Environment variable", "Effect"], [
                [c("BIC_SKIP_DOWNLOAD=1"), "Do not download the binary (offline installs)."],
                [c("BIC_BIN=/path/to/bic"), "Use this binary instead of the downloaded one."],
            ])
            + p(f"Without either, <code>bic</code> from the PATH is used. Binaries for each release are on the <a href='{REPO}/releases'>releases page</a>."),
        },
        {
            "id": "runtime",
            "title": "Decoder API",
            "body": p(f"Import from {c('@proteo5/bic-runtime')}. The functions work in browsers and in Node.js.")
            + "<h3>decodeBic(input, options?)</h3>"
            + p(f"Decodes a BIC file given as a {c('Uint8Array')} or an {c('ArrayBuffer')}. It returns one of two results:")
            + ul([
                f"{c(PIXELS_SHAPE)}: BIC's own coding. {c('rgba')} is a {c('Uint8Array')} of 8-bit RGBA pixels, row by row.",
                f"{c(FILE_SHAPE)}: a PNG or an original JPEG/WebP stored in the file. Show {c('bytes')} as they are.",
            ])
            + p(f"{c('icc')} is the ICC color profile when the file has one, otherwise {c('null')}. Options: {c('maxPixels')} (default 100 million) refuses larger images before any memory is used.")
            + code("""import { decodeBic, BicError } from '@proteo5/bic-runtime';

const bytes = new Uint8Array(await (await fetch('chart.bic')).arrayBuffer());
try {
  const image = decodeBic(bytes);
  if (image.kind === 'pixels') {
    const canvas = document.createElement('canvas');
    canvas.width = image.width;
    canvas.height = image.height;
    canvas.getContext('2d').putImageData(
      new ImageData(new Uint8ClampedArray(image.rgba), image.width, image.height), 0, 0);
    document.body.append(canvas);
  } else {
    const url = URL.createObjectURL(new Blob([image.bytes], { type: image.mime }));
    document.body.insertAdjacentHTML('beforeend', `<img src="${url}" alt="">`);
  }
} catch (error) {
  if (error instanceof BicError) console.error('Not a valid .bic file:', error.message);
  else throw error;
}""")
            + "<h3>Other exports</h3>"
            + ul([
                f"{c('parseContainer(input, options?)')}: the header fields and payload of a file, without decoding the pixels.",
                f"{c('sniffImageType(bytes)')}: {c('image/png')}, {c('image/jpeg')}, {c('image/webp')} or {c('image/heif')}, from the signature; {c('null')} if unknown.",
                f"{c('BicError')}: thrown for malformed or corrupted input. Other exceptions are not expected.",
            ])
            + p("Performance: a 1600-pixel chart decodes in about 20 to 40 ms in JavaScript. The decoder is tested against the Rust decoder on 53 fixtures, and on random corruption."),
        },
        {
            "id": "vite",
            "title": "Vite plugin",
            "body": p(f"The plugin converts the images that pass the policy during {c('vite build')}, and serves them through a Service Worker under their original URLs. Your HTML, CSS and JavaScript do not change.")
            + code("""// vite.config.js
import { bicVite } from '@proteo5/bic-build/vite';

export default {
  plugins: [bicVite({ policy: { minSaving: 0.1, maxDistinctColors: 4096 } })],
};""")
            + p(f"Options: {c('policy')} (see the conversion rule below), and {c('bic')} for the path of the binary. The plugin runs in <code>vite build</code> only.")
            + p("The build writes these files next to your assets:")
            + ul([
                f"{c('bic-sw.js')}: the Service Worker, with the list of converted images built in.",
                f"{c('bic-loader.js')}: registers the worker. It is added to each HTML page before {c('</head>')}.",
                f"{c('bic-report.json')}: the decision and the reason for every image.",
            ])
            + "<h3>Requirements and limits</h3>"
            + ul([
                f"The base path must be absolute ({c('/')}, {c('/app/')}). With a relative base nothing is converted.",
                "Service Workers need HTTPS or localhost.",
                "The first visit downloads the PNG as usual. Later visits show the converted image, and the decoded image is cached, so each .bic is downloaded once per device.",
                "Images inlined as data URLs and files copied from <code>public/</code> are not converted.",
                "A failed conversion keeps the original file and prints a warning; the build does not fail.",
            ]),
        },
        {
            "id": "webpack",
            "title": "webpack plugin",
            "body": p("The same behavior for webpack 5, in production builds. It needs an absolute <code>output.publicPath</code>.")
            + code("""// webpack.config.mjs
import { BicWebpackPlugin } from '@proteo5/bic-build/webpack';

export default {
  mode: 'production',
  output: { publicPath: '/' },
  plugins: [new BicWebpackPlugin({ policy: { minSaving: 0.1 } })],
};""")
            + p(f"Options: {c('policy')}, {c('bic')}, and {c('enableInDevelopment: true')} to convert in development builds too. The loader is added to HTML pages from <code>html-webpack-plugin</code> or another plugin that writes HTML."),
        },
        {
            "id": "policy",
            "title": "Conversion rule",
            "body": p("The build step converts an image only when all of these hold:")
            + ul([
                "The image is not a JPEG. JPEG is lossy, and BIC keeps it as it is.",
                f"It has at most {c('maxDistinctColors')} colors (default 4096). More colors means a photograph-like image.",
                f"The BIC file is at least {c('minSaving')} smaller than the best lossless format: PNG, oxipng or WebP lossless (default {c('0.1')}, that is 10%).",
            ])
            + p("The same rule is available as a function, for your own tools:")
            + code("""import { decide, DEFAULT_POLICY } from '@proteo5/bic-build';

// measurement: the JSON of `bic inspect --json <image>`
const decision = decide(measurement, { minSaving: 0.2 });
// decision.use, decision.reason, decision.saving, decision.best"""),
        },
        {
            "id": "cli",
            "title": "Command line",
            "body": p(f"The {c('bic')} program encodes, decodes, verifies and measures images. The build step uses it for you; these commands are for scripts and manual work.")
            + table(["Command", "What it does"], [
                [c("bic encode <input> <out.bic>"), "Encode an image. Options: <code>--max-error N</code> (near-lossless), <code>--png-fallback</code>, <code>--fast-decode</code>, <code>--transcode</code>, <code>--zstd-level N</code>."],
                [c("bic decode <in.bic> <out>"), "Decode to PNG, JPEG, WebP or BMP, chosen by the extension."],
                [c("bic verify <original> <in.bic>"), "Check that the file decodes to the original pixels. <code>--max-error N</code> accepts bounded differences."],
                [c("bic inspect <image> [--json]"), "Sizes of PNG, oxipng, WebP lossless and BIC for one image."],
                [c("bic compare <paths...> [--csv out.csv]"), "Benchmark BIC against the other lossless formats, on files or folders."],
                [c("bic view <file>"), "Open a .bic or any supported image in the viewer."],
                [c("bic install-association"), "Open .bic files with the viewer for the current Windows user."],
                [c("bic-build <input> <output>"), "Convert a folder with the rule above. Options: <code>--min-saving 10</code>, <code>--max-colors 4096</code>, <code>--bic path</code>."],
            ])
            + code("""# Lossless, verified
bic encode logo.png logo.bic
bic verify logo.png logo.bic

# Near-lossless: no sample differs by more than 2
bic encode photo.png photo.bic --max-error 2

# Convert a folder, writing bic-manifest.json next to the files
npx bic-build src/images dist/bic"""),
        },
        {
            "id": "format",
            "title": "File format",
            "body": p(f"A BIC2 file has a 32-byte header and a payload. The payload is either BIC's own coding (planes, palettes or color transforms, predictors and run-length blocks, compressed with Zstandard), a PNG, or the original JPEG, WebP or HEIF file. The complete specification, with the decoder limits, is in <a href='{REPO}/blob/main/FORMAT.md'>FORMAT.md</a>.")
            + p("The format is experimental. It may change between versions, and the decoders in JavaScript and Rust are kept identical by the tests."),
        },
        {
            "id": "troubleshooting",
            "title": "Troubleshooting",
            "body": table(["Symptom", "Cause and fix"], [
                ["The build prints that the base path is not absolute", f"Set {c('base')} in Vite or {c('output.publicPath')} in webpack to an absolute path such as {c('/')}."],
                ["No image is converted", "Look at <code>bic-report.json</code>: each image has a reason. JPEGs are never converted, and images with more colors than the limit are kept."],
                ["Images are not served as BIC on the first visit", "That is expected: the worker is installed on the first visit. Reload once; later visits use it."],
                ["The page works locally but not on a server", "Service Workers need HTTPS. GitHub Pages, Netlify and similar hosts provide it."],
                [f"{c('bic is older than the bic-build package')}", "The binary has no <code>inspect</code> command. Install a current release, or set <code>BIC_BIN</code>."],
                ["Install prints that the binary could not be downloaded", f"Check the network, or set {c('BIC_BIN')}. The package still works with a binary from the PATH."],
                ["Unsupported browser", "Tested in Chrome. Firefox and Safari are not tested yet."],
            ]),
        },
        {
            "id": "resources",
            "title": "Resources",
            "body": ul([
                f"Source code and issues: <a href='{REPO}'>github.com/proteo5/bic</a>",
                f"Releases and binaries: <a href='{REPO}/releases'>github.com/proteo5/bic/releases</a>",
                "Runtime on npm: <a href='https://www.npmjs.com/package/@proteo5/bic-runtime'>@proteo5/bic-runtime</a>",
                "Build step on npm: <a href='https://www.npmjs.com/package/@proteo5/bic-build'>@proteo5/bic-build</a>",
                f"Measurements and methodology: <a href='{REPO}/blob/main/BENCHMARKS.md'>BENCHMARKS.md</a>",
                f"Changes: <a href='{REPO}/blob/main/CHANGELOG.md'>CHANGELOG.md</a>",
                "License: Apache-2.0",
            ]),
        },
    ],
}

ES = {
    "title": "Documentación de BIC",
    "description": "Instala, usa e integra BIC: el decodificador para navegador, los plugins de Vite y webpack, la línea de comandos y el formato de archivo.",
    "eyebrow": "Documentación",
    "h1": "Documentación de BIC",
    "lead": "Todo lo necesario para usar BIC: los paquetes, su API, los plugins de build, la línea de comandos y el formato. Los ejemplos son código real de los paquetes publicados.",
    "toc_label": "En esta página",
    "sections": [
        {
            "id": "resumen",
            "title": "Resumen",
            "body": p("BIC es un formato de imagen sin pérdida. Guarda gráficas, logotipos, íconos, diagramas y pixel art en menos bytes que un PNG optimizado, y conserva las fotografías como su JPEG original (agregando solo 32 bytes). El proyecto tiene tres partes:")
            + ul([
                f"{c('@proteo5/bic-runtime')}: un decodificador para navegadores y Node.js. Sin dependencias, salvo {c('fzstd')}; 5.7 KiB con gzip.",
                f"{c('@proteo5/bic-build')}: un paso de build para Vite y webpack que convierte las imágenes que se benefician, y un comando {c('bic-build')}.",
                f"{c('bic')}: la herramienta de línea de comandos y el visor, para Windows, Linux y macOS.",
            ])
            + p(f"El formato de archivo está descrito en <a href='{REPO}/blob/main/FORMAT.md'>FORMAT.md</a>."),
        },
        {
            "id": "instalacion",
            "title": "Instalación",
            "body": p("Instala los paquetes que necesites. El decodificador basta para leer archivos {bic}; el paso de build es para proyectos que convierten imágenes.".replace("{bic}", ".bic"))
            + code("npm install @proteo5/bic-runtime\nnpm install --save-dev @proteo5/bic-build")
            + p("El paso de build necesita el programa <code>bic</code>. Al instalarse descarga el binario de tu plataforma desde el release con el mismo número de versión, lo verifica contra las sumas del release y lo guarda en <code>vendor/</code>.")
            + table(["Variable de entorno", "Efecto"], [
                [c("BIC_SKIP_DOWNLOAD=1"), "No descarga el binario (instalaciones sin red)."],
                [c("BIC_BIN=/ruta/a/bic"), "Usa este binario en lugar del descargado."],
            ])
            + p(f"Sin ninguna de las dos, se usa <code>bic</code> del PATH. Los binarios de cada versión están en la <a href='{REPO}/releases'>página de releases</a>."),
        },
        {
            "id": "api-runtime",
            "title": "API del decodificador",
            "body": p(f"Importa desde {c('@proteo5/bic-runtime')}. Las funciones funcionan en navegadores y en Node.js.")
            + "<h3>decodeBic(entrada, opciones?)</h3>"
            + p(f"Decodifica un archivo BIC dado como {c('Uint8Array')} o {c('ArrayBuffer')}. Devuelve uno de dos resultados:")
            + ul([
                f"{c(PIXELS_SHAPE)}: la codificación propia de BIC. {c('rgba')} es un {c('Uint8Array')} de píxeles RGBA de 8 bits, por filas.",
                f"{c(FILE_SHAPE)}: un PNG o un JPEG/WebP original guardado en el archivo. Muestra {c('bytes')} tal cual.",
            ])
            + p(f"{c('icc')} es el perfil de color ICC cuando el archivo lo tiene, si no, {c('null')}. Opciones: {c('maxPixels')} (por defecto 100 millones) rechaza imágenes más grandes antes de usar memoria.")
            + code("""import { decodeBic, BicError } from '@proteo5/bic-runtime';

const bytes = new Uint8Array(await (await fetch('grafica.bic')).arrayBuffer());
try {
  const imagen = decodeBic(bytes);
  if (imagen.kind === 'pixels') {
    const canvas = document.createElement('canvas');
    canvas.width = imagen.width;
    canvas.height = imagen.height;
    canvas.getContext('2d').putImageData(
      new ImageData(new Uint8ClampedArray(imagen.rgba), imagen.width, imagen.height), 0, 0);
    document.body.append(canvas);
  } else {
    const url = URL.createObjectURL(new Blob([imagen.bytes], { type: imagen.mime }));
    document.body.insertAdjacentHTML('beforeend', `<img src="${url}" alt="">`);
  }
} catch (error) {
  if (error instanceof BicError) console.error('No es un archivo .bic válido:', error.message);
  else throw error;
}""")
            + "<h3>Otras exportaciones</h3>"
            + ul([
                f"{c('parseContainer(entrada, opciones?)')}: los campos del encabezado y la carga del archivo, sin decodificar los píxeles.",
                f"{c('sniffImageType(bytes)')}: {c('image/png')}, {c('image/jpeg')}, {c('image/webp')} o {c('image/heif')}, según la firma; {c('null')} si no se reconoce.",
                f"{c('BicError')}: se lanza con entradas mal formadas o dañadas. Otras excepciones no se esperan.",
            ])
            + p("Rendimiento: una gráfica de 1600 píxeles se decodifica en unos 20 a 40 ms en JavaScript. El decodificador se prueba contra el de Rust con 53 casos, y con corrupción aleatoria."),
        },
        {
            "id": "vite",
            "title": "Plugin de Vite",
            "body": p(f"El plugin convierte las imágenes que cumplen la regla durante {c('vite build')}, y las sirve con un Service Worker bajo sus URL originales. Tu HTML, CSS y JavaScript no cambian.")
            + code("""// vite.config.js
import { bicVite } from '@proteo5/bic-build/vite';

export default {
  plugins: [bicVite({ policy: { minSaving: 0.1, maxDistinctColors: 4096 } })],
};""")
            + p(f"Opciones: {c('policy')} (ver la regla de conversión), y {c('bic')} para la ruta del binario. El plugin corre solo en <code>vite build</code>.")
            + p("El build escribe estos archivos junto a tus assets:")
            + ul([
                f"{c('bic-sw.js')}: el Service Worker, con la lista de imágenes convertidas incluida.",
                f"{c('bic-loader.js')}: registra el worker. Se agrega a cada página HTML antes de {c('</head>')}.",
                f"{c('bic-report.json')}: la decisión y el motivo de cada imagen.",
            ])
            + "<h3>Requisitos y límites</h3>"
            + ul([
                f"La ruta base debe ser absoluta ({c('/')}, {c('/app/')}). Con una base relativa no se convierte nada.",
                "Los Service Workers necesitan HTTPS o localhost.",
                "La primera visita descarga el PNG de siempre. Las siguientes muestran la imagen convertida, y la imagen decodificada se guarda en caché, así que cada .bic se descarga una vez por dispositivo.",
                "Las imágenes incrustadas como data URL y los archivos copiados desde <code>public/</code> no se convierten.",
                "Si una conversión falla, se conserva el archivo original y se imprime una advertencia; el build no falla.",
            ]),
        },
        {
            "id": "webpack",
            "title": "Plugin de webpack",
            "body": p("El mismo comportamiento para webpack 5, en builds de producción. Necesita un <code>output.publicPath</code> absoluto.")
            + code("""// webpack.config.mjs
import { BicWebpackPlugin } from '@proteo5/bic-build/webpack';

export default {
  mode: 'production',
  output: { publicPath: '/' },
  plugins: [new BicWebpackPlugin({ policy: { minSaving: 0.1 } })],
};""")
            + p(f"Opciones: {c('policy')}, {c('bic')}, y {c('enableInDevelopment: true')} para convertir también en builds de desarrollo. El loader se agrega a las páginas HTML generadas por <code>html-webpack-plugin</code> u otro plugin que escriba HTML."),
        },
        {
            "id": "regla",
            "title": "Regla de conversión",
            "body": p("El paso de build convierte una imagen solo cuando se cumplen todas estas condiciones:")
            + ul([
                "La imagen no es un JPEG. El JPEG tiene pérdida, y BIC lo conserva tal cual.",
                f"Tiene como máximo {c('maxDistinctColors')} colores (por defecto 4096). Más colores significa una imagen parecida a una fotografía.",
                f"El archivo BIC es al menos {c('minSaving')} más pequeño que el mejor formato sin pérdida: PNG, oxipng o WebP sin pérdida (por defecto {c('0.1')}, es decir 10%).",
            ])
            + p("La misma regla está disponible como función, para tus herramientas:")
            + code("""import { decide, DEFAULT_POLICY } from '@proteo5/bic-build';

// medición: el JSON de `bic inspect --json <imagen>`
const decision = decide(medicion, { minSaving: 0.2 });
// decision.use, decision.reason, decision.saving, decision.best"""),
        },
        {
            "id": "cli",
            "title": "Línea de comandos",
            "body": p(f"El programa {c('bic')} codifica, decodifica, verifica y mide imágenes. El paso de build lo usa por ti; estos comandos son para scripts y trabajo manual.")
            + table(["Comando", "Qué hace"], [
                [c("bic encode <entrada> <salida.bic>"), "Codifica una imagen. Opciones: <code>--max-error N</code> (casi sin pérdida), <code>--png-fallback</code>, <code>--fast-decode</code>, <code>--transcode</code>, <code>--zstd-level N</code>."],
                [c("bic decode <entrada.bic> <salida>"), "Decodifica a PNG, JPEG, WebP o BMP, según la extensión."],
                [c("bic verify <original> <entrada.bic>"), "Comprueba que el archivo se decodifica a los píxeles originales. <code>--max-error N</code> acepta diferencias acotadas."],
                [c("bic inspect <imagen> [--json]"), "Tamaños de PNG, oxipng, WebP sin pérdida y BIC para una imagen."],
                [c("bic compare <rutas...> [--csv salida.csv]"), "Compara BIC con los otros formatos sin pérdida, en archivos o carpetas."],
                [c("bic view <archivo>"), "Abre un .bic o cualquier imagen soportada en el visor."],
                [c("bic install-association"), "Asocia los archivos .bic con el visor para el usuario de Windows actual."],
                [c("bic-build <entrada> <salida>"), "Convierte una carpeta con la regla de arriba. Opciones: <code>--min-saving 10</code>, <code>--max-colors 4096</code>, <code>--bic ruta</code>."],
            ])
            + code("""# Sin pérdida, verificado
bic encode logo.png logo.bic
bic verify logo.png logo.bic

# Casi sin pérdida: ninguna muestra difiere más de 2
bic encode foto.png foto.bic --max-error 2

# Convierte una carpeta y escribe bic-manifest.json junto a los archivos
npx bic-build src/images dist/bic"""),
        },
        {
            "id": "formato",
            "title": "Formato de archivo",
            "body": p(f"Un archivo BIC2 tiene un encabezado de 32 bytes y una carga. La carga es la codificación propia de BIC (planos, paletas o transformadas de color, predictores y bloques de longitud de series, comprimidos con Zstandard), un PNG, o el archivo JPEG, WebP o HEIF original. La especificación completa, con los límites del decodificador, está en <a href='{REPO}/blob/main/FORMAT.md'>FORMAT.md</a>.")
            + p("El formato es experimental. Puede cambiar entre versiones; las pruebas mantienen idénticos los decodificadores de JavaScript y de Rust."),
        },
        {
            "id": "problemas",
            "title": "Solución de problemas",
            "body": table(["Síntoma", "Causa y solución"], [
                ["El build dice que la ruta base no es absoluta", f"Pon {c('base')} en Vite o {c('output.publicPath')} en webpack con una ruta absoluta como {c('/')}."],
                ["No se convierte ninguna imagen", "Revisa <code>bic-report.json</code>: cada imagen tiene un motivo. Los JPEG nunca se convierten, y las imágenes con más colores que el límite se conservan."],
                ["Las imágenes no se sirven como BIC en la primera visita", "Es lo esperado: el worker se instala en la primera visita. Recarga una vez; las siguientes visitas lo usan."],
                ["La página funciona en local pero no en el servidor", "Los Service Workers necesitan HTTPS. GitHub Pages, Netlify y servicios similares lo ofrecen."],
                [c("bic is older than the bic-build package"), "El binario no tiene el comando <code>inspect</code>. Instala un release actual, o usa <code>BIC_BIN</code>."],
                ["La instalación dice que no pudo descargar el binario", f"Revisa la red, o usa {c('BIC_BIN')}. El paquete sigue funcionando con un binario del PATH."],
                ["Navegador no soportado", "Probado en Chrome. Firefox y Safari todavía no están probados."],
            ]),
        },
        {
            "id": "recursos",
            "title": "Recursos",
            "body": ul([
                f"Código fuente y reportes: <a href='{REPO}'>github.com/proteo5/bic</a>",
                f"Releases y binarios: <a href='{REPO}/releases'>github.com/proteo5/bic/releases</a>",
                "Runtime en npm: <a href='https://www.npmjs.com/package/@proteo5/bic-runtime'>@proteo5/bic-runtime</a>",
                "Paso de build en npm: <a href='https://www.npmjs.com/package/@proteo5/bic-build'>@proteo5/bic-build</a>",
                f"Mediciones y metodología: <a href='{REPO}/blob/main/BENCHMARKS.md'>BENCHMARKS.md</a>",
                f"Cambios: <a href='{REPO}/blob/main/CHANGELOG.md'>CHANGELOG.md</a>",
                "Licencia: Apache-2.0",
            ]),
        },
    ],
}

DOCS = {"en": EN, "es": ES}
