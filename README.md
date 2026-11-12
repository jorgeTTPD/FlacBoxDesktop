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



Versión inicial para probar en Linux/X11. Conocido: en GPUI 0.2.2 sobre X11 la posición del puntero se calcula dividiendo por `u16::MAX`, así que los clics pueden caer desplazados; se corrige parcheando `gpui` localmente.
