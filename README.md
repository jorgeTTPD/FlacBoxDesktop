# FlacBox Desktop

Reproductor de música de escritorio (Linux) hecho con [GPUI](https://crates.io/crates/gpui) (la librería de UI de Zed) sobre un motor de audio propio en Rust.

Comparte el mismo motor que la versión Android: decodificación con Symphonia (FLAC, MP3, AAC/M4A, OGG, Opus, WAV), cola, shuffle/repeat, seek y carátulas embebidas leídas con Lofty.

## Funcionalidad

- Biblioteca local: canciones, álbumes y artistas, con búsqueda en vivo.
- Cola de reproducción, aleatorio y repetición (off / todo / una).
- Barra inferior con carátula, título, artista, tiempo y seek.
- Vista "Ahora suena" a pantalla completa: portada (estilo Spotify) con letras sincronizadas al lado en ventanas anchas, y reflow a formato vertical al estrechar la ventana (mosaico bspwm/sway). `l` alterna el modo letras a pantalla completa (estilo Pixel Player) con fondo derivado del color dominante de la carátula.
- Carpetas de música: por defecto `~/Música` y `~/Music`; puedes añadir más y quedan guardadas en `~/.config/flacbox/folders`.
- **Internet** (pestaña, tecla `4`): búsqueda en YouTube vía `yt-dlp`, resultados con miniatura/duración/vistas. Botón ▷ → descarga `bestaudio` al cache (`~/.cache/flacbox/streams`) y suena como archivo local (seek, duración, shuffle, cola, portada de la miniatura y letras LRCLIB). Botón ▣ → descarga el video (`bestvideo+bestaudio` 480p, merge mkv/mp4) y **lo reproduce dentro de «Ahora suena»** (vídeo integrado en la app, audio vía GStreamer). Requiere `yt-dlp` (`pacman -S yt-dlp`); **el video integrado además requiere GStreamer 1.14+** con plugins base/good(+bad/libav para H.264/AV1) (`pacman -S gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav`).
- Nota legal: YouTube no licencia oficialmente el streaming por su API para clientes no oficiales; esto es para uso personal.
- Salida de audio por ALSA/PipeWire vía `cpal`.
- Letra sincronizada en la vista "Ahora suena", con auto-scroll a la línea activa.

## Letra sincronizada

La vista "Ahora suena" (`f`) muestra un recuadro de letra a la derecha, sincronizado con la posición de la pista y con auto-scroll a la línea activa. En ventanas anchas (≥1060 px) conviven portada y letra; en ventana estrecha la letra pasa al modo a pantalla completa. El modo letras a pantalla completa (botón LETRA o tecla `l`, solo con letra sincronizada disponible) ocupa toda la vista con fondo de gradiente derivado del color dominante de la carátula, la línea activa centrada y resaltada, y selector de fuente (Auto / Embebida / .LRC / Cache / Online). En este modo, `Escape` vuelve a la portada.

## Desarrollo

Variables de entorno para pruebas visuales: `FLACBOX_START_VIEW=albums|artists|internet` (pestaña inicial), `FLACBOX_START_PLAYER=1` (abre directamente "Ahora suena"), `FLACBOX_START_MODE=lyrics` (muestra el modo letras), `FLACBOX_START_QUERY="tu búsqueda"` (busca automáticamente en Internet al abrir), `FLACBOX_VIDEO_ID=<id-de-video>` (arranca con ese video de Internet en «Ahora suena», si ya está en cache). Cache de streams: `~/.cache/flacbox/streams` (o `FLACBOX_STREAMS_DIR`).

Orden de resolución (mismo criterio que los scripts de Python de `lyrics-on-panel` y `repMusic`, portado a Rust):

1. Letra embebida sincronizada en las etiquetas (`USLT`/`©lyr`, vía `lofty`).
2. Archivo `.lrc` junto a la pista (`cancion.flac` → `cancion.lrc`).
3. Cache en disco (`~/.cache/flacbox/lyrics`, o `FLACBOX_LYRICS_CACHE`).
4. LRCLIB: coincidencia exacta con duración → búsqueda → búsqueda difusa.
5. Letra embebida sin sincronizar, como texto plano.

- `core/src/lyrics.rs` — parser LRC (varios timestamps por línea), línea activa, sidecar y etiquetas. Sin red.
- `desktop/src/lyrics.rs` — fetch a LRCLIB con `ureq`, cache JSON y resolución en un hilo aparte (mismo patrón que el escaneo de biblioteca).

## Estado

Versión inicial para probar en Linux/X11. Nota: GPUI 0.2.2 sobre X11 calcula la posición del puntero dividiendo por `u16::MAX` en vez de 65536.0 (`Fp1616`) en `src/platform/linux/x11/client.rs`. El error es subpíxel en cualquier escala práctica, así que no es causa de clics mal ubicados; 0.2.2 es la última versión publicada. Si llegara a molestar, la corrección es vendorizar `gpui` localmente con `[patch.crates-io]` y cambiar el divisor.
