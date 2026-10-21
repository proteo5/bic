# BIC

[![CI](https://github.com/proteo5/bic/actions/workflows/ci.yml/badge.svg)](https://github.com/proteo5/bic/actions/workflows/ci.yml)
[![npm: @proteo5/bic-runtime](https://img.shields.io/npm/v/@proteo5/bic-runtime?label=bic-runtime)](https://www.npmjs.com/package/@proteo5/bic-runtime)
[![npm: @proteo5/bic-build](https://img.shields.io/npm/v/@proteo5/bic-build?label=bic-build)](https://www.npmjs.com/package/@proteo5/bic-build)
[![Licencia: Apache-2.0](https://img.shields.io/badge/licencia-Apache--2.0-blue.svg)](LICENSE)

**BIC es un formato de imagen sin pérdida para gráficos planos.** Gráficas,
logotipos, iconos, diagramas y pixel art ocupan en muchos casos menos bytes que
un PNG optimizado. Las fotografías se conservan como su JPEG original, así que ni
ganan ni pierden.

- **Documentación, ejemplos en vivo y resultados:** <https://proteo5.github.io/bic/>
- **English:** [README.md](README.md)

El proyecto tiene tres partes:

| Parte | Qué es | Dónde |
| --- | --- | --- |
| `bic` | Herramienta de línea de comandos y visor para Windows, Linux y macOS | [Releases](https://github.com/proteo5/bic/releases) |
| `@proteo5/bic-runtime` | Decodificador para navegadores y Node.js (5.7 KiB comprimido con gzip, una dependencia) | [npm](https://www.npmjs.com/package/@proteo5/bic-runtime) |
| `@proteo5/bic-build` | Paso de construcción y plugins para Vite y webpack, más el comando `bic-build` | [npm](https://www.npmjs.com/package/@proteo5/bic-build) |

> **Estado:** experimental. El formato puede cambiar entre versiones. Cada
> archivo se verifica para decodificar exactamente los píxeles originales antes
> de publicarse.

## Cuándo usar BIC

| Contenido | BIC frente a PNG optimizado |
| --- | --- |
| Gráficas de barras y diagramas con áreas sólidas | **Más pequeño**: 46 % en el ejemplo de la gráfica de barras |
| Iconos y logotipos con pocos colores | **Igual o más pequeño** (icono: 0.2 % menos) |
| Capturas de software con texto suavizado | Más grande (+19 %): miles de colores |
| Diagramas de flujo y pixel art | Más grande (+12 % a +15 %) |
| Fotografías | Sin ganancia: el JPEG se conserva tal cual (+32 bytes) |

Los formatos con pérdida, como WebP y AVIF, suelen ser mucho más pequeños que
cualquier formato sin pérdida, pero cambian los valores de los píxeles. Úsalos
cuando no necesites píxeles exactos. El método y las tablas completas están en
[BENCHMARKS.md](BENCHMARKS.md) y en el [sitio de documentación](https://proteo5.github.io/bic/).

## Inicio rápido

### Decodificar un archivo .bic en el navegador o en Node.js

```bash
npm install @proteo5/bic-runtime
```

```js
import { decodeBic, BicError } from '@proteo5/bic-runtime';

const bytes = new Uint8Array(await (await fetch('grafica.bic')).arrayBuffer());
try {
  const image = decodeBic(bytes);
  if (image.kind === 'pixels') {
    // image.width, image.height, image.rgba: RGBA de 8 bits, fila por fila
  } else {
    // image.mime y image.bytes: un PNG o un JPEG/WebP original, se muestran tal cual
  }
} catch (error) {
  if (error instanceof BicError) console.error(error.message);
  else throw error;
}
```

### Usar el paso de construcción en un proyecto de Vite o webpack

```bash
npm install --save-dev @proteo5/bic-build
```

```js
// vite.config.js
import { bicVite } from '@proteo5/bic-build/vite';

export default { plugins: [bicVite()] };
```

```js
// webpack.config.mjs (compilaciones de producción; publicPath debe ser absoluta)
import { BicWebpackPlugin } from '@proteo5/bic-build/webpack';

export default {
  mode: 'production',
  output: { publicPath: '/' },
  plugins: [new BicWebpackPlugin()],
};
```

La construcción convierte una imagen solo cuando el archivo BIC pesa al menos
10 % menos que el mejor formato sin pérdida, y nunca convierte un JPEG ni una
imagen con más de 4096 colores. Tu HTML y tu CSS no cambian: un Service Worker
responde a las URLs originales de las imágenes con la imagen convertida. Sin el
worker, el navegador descarga el PNG como siempre. La referencia está en la
[documentación](https://proteo5.github.io/bic/docs/).

### Usar la línea de comandos

Descarga la versión para tu sistema desde la [página de releases](https://github.com/proteo5/bic/releases),
o compílala desde el código fuente (ver [Desarrollo](#desarrollo)).

```bash
bic encode logo.png logo.bic                   # sin pérdida
bic verify logo.png logo.bic                   # los píxeles decodificados deben coincidir exactamente
bic decode logo.bic logo-copia.png             # de vuelta a PNG
bic inspect logo.png --json                    # tamaños de PNG, oxipng, WebP y BIC
bic encode foto.png foto.bic --max-error 2     # casi sin pérdida: ninguna muestra cambia más de 2
npx bic-build src/imagenes dist/bic            # convierte una carpeta y escribe bic-manifest.json
```

Ejecuta `bic --help` para ver todas las opciones.

## Resultados

Medidos sobre las imágenes de ejemplo del [sitio de documentación](https://proteo5.github.io/bic/).
Todas son imágenes de dominio público o con licencia Creative Commons de Wikimedia Commons.
Tamaños en KiB. La referencia es oxipng, el formato sin pérdida más pequeño en cada ejemplo.

| Ejemplo | Píxeles | oxipng | BIC | BIC frente a oxipng |
| --- | ---: | ---: | ---: | ---: |
| Gráfica de barras | 1600×800 | 87.7 | **47.3** | **46 % más pequeño** |
| Icono (transparente) | 512×512 | 7.3 | 7.3 | 0.2 % más pequeño |
| Interfaz de software | 1600×859 | 114.5 | 136.3 | 19 % más grande |
| Pixel art | 800×480 | 77.2 | 89.0 | 15 % más grande |
| Diagrama de flujo | 706×905 | 10.0 | 11.1 | 12 % más grande |
| Fotografía (se conserva el JPEG) | 1600×1064 | — | 492.5 | el JPEG más 32 bytes |

En el corpus sintético de referencia, la media geométrica de BIC frente a oxipng
es 0.755 (menor es más pequeño). En un conjunto privado de fotografías, la
codificación sin pérdida da 0.837. Ambos conjuntos y el método están en
[BENCHMARKS.md](BENCHMARKS.md).

## Cómo funciona

- Las imágenes con como máximo 256 colores pueden guardarse como paleta. Si no, al canal rojo y al azul se les puede restar el verde. Cada fila puede usar su propio predictor (none, left, up, average, Paeth o MED).
- Los planos se codifican como bloques de longitud de series y se comprimen con Zstandard. El codificador prueba un conjunto pequeño de estas combinaciones y conserva la más pequeña. La elección de candidatos se explica en [BENCHMARKS.md](BENCHMARKS.md).
- El modo casi sin pérdida (`--max-error N`) acota el error de cada muestra.
- Un archivo puede incluir el PNG, JPEG, WebP o HEIF original. Así, un `.bic` nunca pesa más que su origen más 32 bytes.
- Los decodificadores revisan tamaños y longitudes antes de reservar memoria. Una entrada mal formada devuelve un error.

La especificación a nivel de bytes está en [FORMAT.md](FORMAT.md).

## Paquetes y versiones

| Paquete | Registro | Última versión |
| --- | --- | --- |
| `bic` (biblioteca del códec y CLI) | [crates.io](https://crates.io/crates/bic) | 0.2.0 en crates.io; 0.2.1 en este repositorio |
| Binarios `bic` y visor `bic-view` | [releases de GitHub](https://github.com/proteo5/bic/releases) | 0.2.1 |
| `@proteo5/bic-runtime` | [npm](https://www.npmjs.com/package/@proteo5/bic-runtime) | 0.2.1 |
| `@proteo5/bic-build` | [npm](https://www.npmjs.com/package/@proteo5/bic-build) | 0.2.1 |

Al instalarse, el paso de construcción descarga el binario `bic` para tu
plataforma (Windows x64, Linux x64, macOS Apple Silicon e Intel) y lo verifica
con las sumas de comprobación del release. Define `BIC_SKIP_DOWNLOAD=1` para
omitir la descarga, o `BIC_BIN` para usar tu propio binario.

## Referencia de la línea de comandos

| Comando | Qué hace |
| --- | --- |
| `bic encode <entrada> <salida.bic>` | Codifica una imagen. Opciones: `--max-error N`, `--png-fallback`, `--fast-decode`, `--transcode`, `--zstd-level N` |
| `bic decode <entrada.bic> <salida>` | Decodifica a un archivo de imagen; el formato sigue la extensión. La salida PNG conserva el perfil ICC |
| `bic verify <original> <archivo.bic> [--max-error N]` | Comprueba que el archivo decodifica a los píxeles originales (o con diferencia máxima `N`) |
| `bic inspect <imagen> [--json]` | Tamaños de PNG, oxipng, WebP sin pérdida y BIC para una imagen |
| `bic compare <rutas>... [--csv salida.csv]` | Compara BIC con los otros formatos sin pérdida, en archivos o carpetas |
| `bic view <archivo>` | Abre un `.bic` o cualquier imagen compatible en el visor |
| `bic-view [archivo]` | El mismo visor sin ventana de consola |
| `bic install-association` | Abre los `.bic` con el visor para el usuario actual de Windows |
| `bic-build <entrada> <salida>` | Convierte una carpeta con la regla de construcción. Opciones: `--min-saving 10`, `--max-colors 4096`, `--bic <ruta>` |

## Desarrollo

Requisitos: un toolchain de Rust estable y reciente para el códec. Los paquetes de
JavaScript necesitan Node.js 20 o posterior; el CI usa Node.js 22.

```bash
cargo test                                       # pruebas del códec, de la CLI y basadas en propiedades
cargo clippy --all-targets -- -D warnings
cargo build --release --bin bic                  # la herramienta de línea de comandos

cd packages/bic-runtime && npm ci && npm test    # decodificador, verificado con 53 fixtures escritos por el códec de Rust
cd packages/bic-build && npm ci && npm test      # paso de construcción: conversión, verificación, instalador del binario
```

El sitio se construye con `python site/build.py` después de `npm ci` dentro de
`site/`. Necesita Pillow y una compilación de publicación de `bic`. La prueba de
extremo a extremo en el navegador se ejecuta en Chrome cuando `BIC_CHROME` apunta
a él (ver `packages/bic-build/test/browser.test.mjs`).

### Estructura del repositorio

| Ruta | Contenido |
| --- | --- |
| `src/` | La biblioteca del códec, la herramienta `bic` y el visor `bic-view` |
| `packages/bic-runtime/` | El decodificador de JavaScript (`@proteo5/bic-runtime`) y sus fixtures de conformidad |
| `packages/bic-build/` | El paso de construcción y los plugins de Vite y webpack (`@proteo5/bic-build`) |
| `site/` | El sitio de documentación y demostración (Python y esbuild) |
| `fuzz/` | Objetivos de cargo-fuzz para el decodificador |
| `spike/` | Los experimentos del navegador detrás del diseño: Service Worker y WebAssembly |
| `examples/` | `gen_corpus` (imágenes sintéticas de referencia) y `dump_rgba` (píxeles de referencia para las pruebas) |

### Fuzzing

El crate `fuzz/` tiene tres objetivos de [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz):
`decode` (archivos completos), `decode_plain` (el flujo de planos dentro de un
contenedor válido) y `roundtrip` (imágenes y opciones arbitrarias). El CI ejecuta
cada uno durante dos minutos en cada push.

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run decode_plain -- -max_total_time=300
```

En Windows (MSVC) el runtime de AddressSanitizer debe estar en el `PATH`, por
ejemplo `VC\Tools\MSVC\<versión>\bin\Hostx64\x64` de tu instalación de Visual Studio.

## Límites

- El códec trabaja con RGBA de 8 bits. Las profundidades mayores se reducen a 8 bits al leerse.
- Los perfiles ICC se escriben solo en la salida PNG. Los demás formatos de salida los descartan y muestran una nota.
- Los Service Workers necesitan HTTPS o `localhost`. Chrome está probado de extremo a extremo; Firefox y Safari todavía no están probados.
- La codificación es lenta: el codificador prueba varios candidatos, y una fotografía de 20 megapíxeles tarda cerca de un minuto. Decodificar gráficos planos toma unos milisegundos.

## Contribuir

Las incidencias y los pull requests son bienvenidos. Para cambios en el códec,
incluye pruebas y, si cambian la compresión, los tamaños antes y después sobre
imágenes representativas. La lista de verificación que ejecuta el CI está en los
archivos de flujo bajo `.github/workflows/`.

## Licencia

Licenciado bajo la [Licencia Apache, versión 2.0](LICENSE). Salvo que indiques lo
contrario, cualquier contribución que envíes se rige por los mismos términos.
