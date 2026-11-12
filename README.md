# FlacBox Desktop

Reproductor de música de escritorio (Linux) hecho con [GPUI](https://crates.io/crates/gpui) (la librería de UI de Zed) sobre un motor de audio propio en Rust.

Comparte el mismo motor que la versión Android: decodificación con Symphonia (FLAC, MP3, AAC/M4A, OGG, Opus, WAV), cola, shuffle/repeat, seek y carátulas embebidas leídas con Lofty.

## Funcionalidad

- Biblioteca local: canciones, álbumes y artistas, con búsqueda en vivo.
- Cola de reproducción, aleatorio y repetición (off / todo / una).
- Barra inferior con carátula, título, artista, tiempo y seek.
- Vista "Ahora suena" a pantalla completa.
- Carpetas de música: por defecto `~/Música` y `~/Music`; puedes añadir más y quedan guardadas en `~/.config/flacbox/folders`.
- Salida de audio por ALSA/PipeWire vía `cpal`.
- Letra sincronizada en la vista "Ahora suena", con auto-scroll a la línea activa.

## Letra sincronizada

La vista "Ahora suena" (`f`) muestra un recuadro de letra a la derecha, sincronizado con la posición de la pista y con auto-scroll a la línea activa.

Orden de resolución (mismo criterio que los scripts de Python de `lyrics-on-panel` y `repMusic`, portado a Rust):

1. Letra embebida sincronizada en las etiquetas (`USLT`/`©lyr`, vía `lofty`).
2. Archivo `.lrc` junto a la pista (`cancion.flac` → `cancion.lrc`).
3. Cache en disco (`~/.cache/flacbox/lyrics`, o `FLACBOX_LYRICS_CACHE`).
4. LRCLIB: coincidencia exacta con duración → búsqueda → búsqueda difusa.
5. Letra embebida sin sincronizar, como texto plano.

- `core/src/lyrics.rs` — parser LRC (varios timestamps por línea), línea activa, sidecar y etiquetas. Sin red.
- `desktop/src/lyrics.rs` — fetch a LRCLIB con `ureq`, cache JSON y resolución en un hilo aparte (mismo patrón que el escaneo de biblioteca).

## Estado

Versión inicial para probar en Linux/X11. Conocido: en GPUI 0.2.2 sobre X11 la posición del puntero se calcula dividiendo por `u16::MAX`, así que los clics pueden caer desplazados; se corrige parcheando `gpui` localmente.
