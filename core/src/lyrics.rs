use std::path::{Path, PathBuf};

use lofty::file::TaggedFileExt;
use lofty::tag::ItemKey;

/// Origen de la letra de una pista.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LyricSource {
    /// Sin datos.
    #[default]
    Unknown,
    /// Etiquetas embebidas en el archivo (USLT/`©lyr`).
    Embedded,
    /// Archivo `.lrc` junto a la pista.
    Sidecar,
    /// Cache local de lo bajado de internet.
    Lrclib,
    /// Texto plano sin sincronizar.
    Plain,
}

/// Una línea de letra. `time_ms` es 0 si la letra no está sincronizada.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
}

/// Letra completa de una pista.
#[derive(Clone, Debug, Default)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub synced: bool,
    pub source: LyricSource,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Índice de la línea que suena en la posición dada.
    pub fn active_index(&self, position_ms: i64) -> Option<usize> {
        active_index(&self.lines, position_ms)
    }
}

/// Convierte texto plano en líneas sin sincronizar.
pub fn plain_lines(text: &str) -> Vec<LyricLine> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| LyricLine {
            time_ms: 0,
            text: l.to_string(),
        })
        .collect()
}

/// Parsea un timestamp `[mm:ss]`, `[mm:ss.xx]` o `[mm:ss.xxx]`.
fn parse_timestamp(s: &str) -> Option<u64> {
    let (min, rest) = s.split_once(':')?;
    if min.is_empty() || !min.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (sec, frac) = match rest.split_once('.') {
        Some((a, b)) => (a, b),
        None => (rest, ""),
    };
    if sec.is_empty() || !sec.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !frac.is_empty() && !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let min: u64 = min.parse().ok()?;
    let sec: u64 = sec.parse().ok()?;
    let ms = match frac.len() {
        0 => 0,
        1 => frac.parse::<u64>().ok()? * 100,
        2 => frac.parse::<u64>().ok()? * 10,
        3 => frac.parse::<u64>().ok()?,
        _ => frac.get(..3)?.parse().ok()?,
    };

    Some((min * 60 + sec) * 1000 + ms)
}

/// Parsea un LRC completo. Devuelve las líneas y si tenía al menos un timestamp.
///
/// Admite varios timestamps por línea (`[00:12.00][01:30.00]Texto`) e ignora
/// metadatos tipo `[ar:Artista]`. Las líneas sin timestamp se descartan.
pub fn parse_lrc(text: &str) -> (Vec<LyricLine>, bool) {
    let mut out: Vec<LyricLine> = Vec::new();
    let mut timed = false;

    for raw in text.lines() {
        let mut times: Vec<u64> = Vec::new();
        let mut rest = raw;
        loop {
            let Some(open) = rest.find('[') else { break };
            let Some(close_rel) = rest[open..].find(']') else {
                break;
            };
            let inner = &rest[open + 1..open + close_rel];
            match parse_timestamp(inner) {
                Some(ms) => {
                    times.push(ms);
                    timed = true;
                }
                None => break,
            }
            rest = &rest[open + close_rel + 1..];
        }

        if times.is_empty() {
            continue;
        }

        let text_line = rest.trim();
        for ms in times {
            out.push(LyricLine {
                time_ms: ms,
                text: text_line.to_string(),
            });
        }
    }

    out.sort_by_key(|l| l.time_ms);
    (out, timed)
}

/// Índice de la última línea cuyo tiempo ya pasó (búsqueda binaria).
/// Las líneas vacías se saltan hacia atrás, como en tus scripts de Python.
pub fn active_index(lines: &[LyricLine], position_ms: i64) -> Option<usize> {
    if lines.is_empty() || position_ms < 0 {
        return None;
    }
    let pos = position_ms as u64;

    // Primer índice con tiempo > pos.
    let mut lo = 0usize;
    let mut hi = lines.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if lines[mid].time_ms <= pos {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo == 0 {
        return None;
    }

    let mut idx = lo - 1;
    while idx > 0 && lines[idx].text.trim().is_empty() {
        idx -= 1;
    }
    if lines[idx].text.trim().is_empty() {
        return None;
    }
    Some(idx)
}

/// Ruta del `.lrc` hermano de una pista (`cancion.flac` -> `cancion.lrc`).
pub fn sidecar_path(audio: &Path) -> PathBuf {
    audio.with_extension("lrc")
}

/// Letra del archivo `.lrc` que acompaña a la pista, si existe.
pub fn read_sidecar(audio: &Path) -> Option<Lyrics> {
    let path = sidecar_path(audio);
    let text = std::fs::read_to_string(path).ok()?;
    let (lines, timed) = parse_lrc(&text);
    if lines.is_empty() {
        return None;
    }
    Some(Lyrics {
        lines,
        synced: timed,
        source: LyricSource::Sidecar,
    })
}

/// Letra embebida en las etiquetas del archivo (sincronizada si es LRC).
pub fn embedded_lyrics(audio: &Path) -> Option<Lyrics> {
    let tagged = lofty::read_from_path(audio).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;

    if let Some(text) = tag.get_string(ItemKey::Lyrics) {
        let (lines, timed) = parse_lrc(text);
        if !lines.is_empty() && timed {
            return Some(Lyrics {
                lines,
                synced: true,
                source: LyricSource::Embedded,
            });
        }
    }

    if let Some(text) = tag.get_string(ItemKey::UnsyncLyrics) {
        let lines = plain_lines(text);
        if !lines.is_empty() {
            return Some(Lyrics {
                lines,
                synced: false,
                source: LyricSource::Embedded,
            });
        }
    }

    None
}

/// Clave de cache estable para una pista.
pub fn cache_key(title: &str, artist: &str, album: &str) -> String {
    format!("{title}|{artist}|{album}")
}

/// Directorio de cache de letras (`~/.cache/flacbox/lyrics`).
pub fn cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("FLACBOX_LYRICS_CACHE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .or_else(|| std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from))?;
    Some(base.join("flacbox").join("lyrics"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lrc(times: &[(u64, &str)]) -> Vec<LyricLine> {
        times
            .iter()
            .map(|(ms, t)| LyricLine {
                time_ms: *ms,
                text: t.to_string(),
            })
            .collect()
    }

    #[test]
    fn parsea_un_timestamp_por_linea() {
        let text = "[00:00.07] They know what is what\n[00:02.07] But they don't know what is what";
        let (lines, timed) = parse_lrc(text);
        assert!(timed);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].time_ms, 70);
        assert_eq!(lines[0].text, "They know what is what");
        assert_eq!(lines[1].time_ms, 2070);
    }

    #[test]
    fn parsea_varios_timestamps_en_una_linea() {
        let (lines, timed) = parse_lrc("[00:10.00][01:30.00] estribillo");
        assert!(timed);
        assert_eq!(
            lines,
            lrc(&[(10_000, "estribillo"), (90_000, "estribillo")])
        );
    }

    #[test]
    fn ignora_metadatos_y_lineas_sin_tiempo() {
        let text = "[ar:Artista]\n[al:Album]\n linea suelta\n[00:05.00] hola";
        let (lines, timed) = parse_lrc(text);
        assert!(timed);
        assert_eq!(lines, lrc(&[(5_000, "hola")]));
    }

    #[test]
    fn timestamp_sin_fraccion_y_con_tres_decimales() {
        let (a, _) = parse_lrc("[01:05] uno");
        let (b, _) = parse_lrc("[01:05.250] dos");
        assert_eq!(a[0].time_ms, 65_000);
        assert_eq!(b[0].time_ms, 65_250);
    }

    #[test]
    fn active_index_elige_la_ultima_linea_pasada() {
        let lines = lrc(&[(0, "a"), (5_000, "b"), (10_000, "c")]);
        assert_eq!(active_index(&lines, -10), None);
        assert_eq!(active_index(&lines, 0), Some(0));
        assert_eq!(active_index(&lines, 4_999), Some(0));
        assert_eq!(active_index(&lines, 5_000), Some(1));
        assert_eq!(active_index(&lines, 999_999), Some(2));
    }

    #[test]
    fn active_index_salta_lineas_vacias() {
        let lines = lrc(&[(0, "a"), (5_000, ""), (10_000, "c")]);
        // A los 7 s seguimos en el hueco instrumental: se mantiene la última con texto.
        assert_eq!(active_index(&lines, 7_000), Some(0));
        assert_eq!(active_index(&lines, 4_000), Some(0));
        assert_eq!(active_index(&lines, 10_000), Some(2));
    }

    #[test]
    fn texto_plano_no_esta_sincronizado() {
        let lines = plain_lines("\n primera \n\nsegunda\n");
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.time_ms == 0));
        assert_eq!(lines[0].text, "primera");
    }

    #[test]
    fn cache_key_es_determinista() {
        assert_eq!(
            cache_key("Hola", "Yo", "Disco"),
            cache_key("Hola", "Yo", "Disco")
        );
        assert_ne!(
            cache_key("Hola", "Yo", "Disco"),
            cache_key("Hola", "Tu", "Disco")
        );
    }
}
