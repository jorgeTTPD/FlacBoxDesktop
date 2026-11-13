# FlacBox Desktop

Reproductor de música para escritorio Linux hecho con GPUI (la librería de UI de Zed)
sobre un motor de audio propio en Rust.

Un FlacBox real en tu máquina: tu biblioteca local FLAC/MP3/AAC/OGG/Opus/WAV con letras
sincronizadas, YouTube integrado (búsqueda, caché y reproducción como archivo local, audio y
vídeo), búsqueda en vivo, colas y carátulas. Sin cuenta, sin publicidad, sin nube.
Tu música, tu portátil.


## Características

- Biblioteca local completa: canciones, álbumes y artistas, búsqueda en vivo (Esc limpia),
  escaneo incremental con barra de estado.
- Cola de reproducción, aleatorio y repetición (off / todo / una).
- Vista «Ahora suena» a pantalla completa: portada grande con estética dark tipo Spotify,
  letras sincronizadas al lado en ventanas anchas, y reflow a formato vertical en ventanas
  estrechas. Tecla `l` alterna un modo letras a pantalla completa estilo Pixel Player
  (fondo derivado del color dominante de la carátula, línea activa centrada y resaltada).
- Letras con orden de resolución razonable: embebida (USLT), luego .lrc junto a la pista,
  luego caché en disco, luego LRCLIB (exacta y aproximada), luego texto plano embebido.
- Internet (tecla `4`): búsqueda en YouTube mediante yt-dlp, con miniatura, título, canal,
  duración y vistas. Dos botones por resultado:

  - ▷  bestaudio: descarga a la caché en ~/.cache/flacbox/streams y suena como archivo
     local (seek, duración, shuffle, cola, portada de la miniatura y letras LRCLIB).
  - ▣  video 480p fusionado: lo reproduce dentro de la app mediante gpui-video-player con
     GStreamer. La letra queda al lado del vídeo y el selector AUDIO / VIDEO permite
     cambiar de un click entre el audio y el vídeo del mismo resultado.

- Modo vídeo completo: seek, play/pausa y tiempos con controles reales de GStreamer;
  Esc vuelve a la portada.
- Barra inferior siempre: carátula, título, artista, tiempo y seek interactivos.
- Carpetas personalizadas: por defecto ~/Música y ~/Music; añade más (guardadas en
  ~/.config/flacbox/folders).


## Requisitos del sistema

Para ejecutar (runtime):

- Linux con ALSA o PipeWire (la salida de audio es vía cpal).
- Opcional, para el modo vídeo y la búsqueda de Internet:
  - yt-dlp (pacman -S yt-dlp, o pip install yt-dlp) — búsqueda y descarga de audio/vídeo.
  - GStreamer 1.14+ con plugins base/good (+bad/libav para AV1 y H.264):
    pacman -S gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav

Para compilar (build):

- Rust estable y cargo.
- pkgconf y los encabezados de ALSA (alsa-lib) y de GStreamer (los paquetes de arriba
  incluyen lo necesario en Arch; en Fedora/Debian instala los dev de los mismos).
- FFmpeg en el PATH solo si quieres el modo vídeo (yt-dlp lo emplea para fusionar
  audio/vídeo; aquí lo tienes al instalar yt-dlp con los plugins anteriores).


## Compilar e instalar

Clona y compila en modo release:

    git clone https://github.com/jorgeTTPD/FlacBoxDesktop.git
    cd FlacBoxDesktop/desktop
    cargo build --release

El binario queda en desktop/target/release/flacbox-desktop. Cópialo donde quieras
(por ejemplo ~/.local/bin) o crea un lanzador .desktop apuntando a él.

Ejecución directa:

    cargo run --release


## Uso

  Canciones / Álbumes / Artistas / Internet   pestañas principales (teclas 1 2 3 4)
  f                                            abre/cierra «Ahora suena»
  l                                            alterna modo letras en «Ahora suena» (si hay letra)
  Space                                        play / pausa
  → / ←                                        seek ±5 s
  n / p                                        siguiente / anterior
  Esc                                          cierra el vídeo de Internet, vuelve a la
                                               portada, sale del modo letras o limpia la búsqueda
  Enter (en Internet)                          lanza la búsqueda
  Ctrl/Cmd+V                                   pega en los campos de texto


## Rutas de datos

  Carpetas extra         ~/.config/flacbox/folders
  Caché de búsqueda YT   ~/.cache/flacbox/streams  (o FLACBOX_STREAMS_DIR)
  Miniaturas             ~/.cache/flacbox/thumbs
  Caché de letras        ~/.cache/flacbox/lyrics   (o FLACBOX_LYRICS_CACHE)
  Estado / sesión        ~/.config/flacboxdesktop/state.json

Variables de entorno para desarrollo o pruebas: FLACBOX_START_VIEW=albums|artists|internet
FLACBOX_START_PLAYER=1, FLACBOX_START_MODE=lyrics, FLACBOX_START_QUERY="tu búsqueda",
FLACBOX_VIDEO_ID=<id-del-video>.


## Desarrollo

Arquitectura en dos crates:

  core/      motor propio (flacplayer): decodificación Symphonia, cola, engine de audio
             sobre cpal, letras LRC, caché de letras, tags/portadas (lofty).
  desktop/   la aplicación GPUI: UI, vistas, Internet, vídeo integrado. Comparte el
             motor con la versión Android.

Compilar en debug:

    cd desktop && cargo run

Compilar en release:

    cd desktop && cargo build --release


## Aviso legal

YouTube no licencia oficialmente el streaming por su API para clientes no oficiales.
La búsqueda/descarga/caché de YouTube de esta aplicación es para uso personal y no
afilia esta sesión a YouTube, a Google ni a Alphabet. No hay DRM de contenido premium;
el acceso es exclusivamente a flujos públicos. Cualquier uso fuera de ese marco
corresponde a ti.


## Contribuciones

Issues y pull requests son bienvenidos. Antes de abrir uno, deja que la biblioteca
escanee completa, reproduce alguna pista y reproduce algún vídeo desde la pestaña
Internet (o con FLACBOX_VIDEO_ID).


## Licencia

MIT (ver el fichero LICENSE): usa, copia, modifica, distribuye (respetando el copyright
y la atribución). Sin garantía.
