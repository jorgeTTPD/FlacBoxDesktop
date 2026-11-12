use std::fs::File;
use std::path::Path;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::{Accessor, ItemKey};

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn field(name: &str, value: Option<&str>) -> String {
    match value {
        Some(v) if !v.trim().is_empty() => format!("\"{}\":\"{}\"", name, esc(v)),
        _ => format!("\"{}\":null", name),
    }
}

fn num_field(name: &str, value: Option<&str>) -> String {
    let parsed = value
        .and_then(|v| v.trim().parse::<u32>().ok())
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".into());
    format!("\"{}\":{}", name, parsed)
}

/// Lee los tags con lofty y devuelve un JSON de una línea.
pub fn read_meta(path: &str) -> String {
    let mut file = match File::open(Path::new(path)) {
        Ok(f) => f,
        Err(_) => return "{}".to_string(),
    };

    let tagged = match lofty::read_from(&mut file) {
        Ok(t) => t,
        Err(_) => return "{\"ok\":false}".to_string(),
    };

    let props = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let duration_ms = props.duration().as_millis() as u64;
    let has_art = tag.map(|t| !t.pictures().is_empty()).unwrap_or(false);

    let title = tag.and_then(|t| t.title());
    let artist = tag.and_then(|t| t.artist());
    let album = tag.and_then(|t| t.album());
    let album_artist = tag.and_then(|t| t.get_string(ItemKey::AlbumArtist));
    let genre = tag.and_then(|t| t.genre());
    let year = tag.and_then(|t| t.get_string(ItemKey::Year));
    let disc = tag.and_then(|t| t.get_string(ItemKey::DiscNumber));
    let track = tag.and_then(|t| t.track()).map(|v| v.to_string());
    let picture_mime = tag
        .and_then(|t| t.pictures().first())
        .and_then(|p| p.mime_type())
        .map(|m| m.to_string());

    let mut json = String::from("{");
    json.push_str("\"ok\":true,");
    json.push_str(&format!(
        "\"durationMs\":{},\"sampleRate\":{},\"channels\":{},\"bitDepth\":{},\"hasArt\":{},",
        duration_ms,
        props.sample_rate().unwrap_or(0),
        props.channels().unwrap_or(0),
        props.bit_depth().unwrap_or(0),
        has_art
    ));
    json.push_str(&field("title", title.as_deref()));
    json.push(',');
    json.push_str(&field("artist", artist.as_deref()));
    json.push(',');
    json.push_str(&field("album", album.as_deref()));
    json.push(',');
    json.push_str(&field("albumArtist", album_artist.as_deref()));
    json.push(',');
    json.push_str(&field("genre", genre.as_deref()));
    json.push(',');
    json.push_str(&num_field("year", year.as_deref()));
    json.push(',');
    json.push_str(&num_field("track", track.as_deref()));
    json.push(',');
    json.push_str(&num_field("disc", disc.as_deref()));
    json.push(',');
    json.push_str("\"pictureMime\":");
    match picture_mime.as_deref() {
        Some(m) => json.push_str(&format!("\"{}\"", esc(m))),
        None => json.push_str("null"),
    }
    json.push('}');
    json
}

/// Extrae la primera carátula incrustada al fichero indicado.
pub fn extract_art(path: &str, out_path: &str) -> bool {
    let mut file = match File::open(Path::new(path)) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let tagged = match lofty::read_from(&mut file) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let tag = match tagged.primary_tag().or_else(|| tagged.first_tag()) {
        Some(t) => t,
        None => return false,
    };
    let picture = match tag.pictures().first() {
        Some(p) => p,
        None => return false,
    };
    if let Some(parent) = Path::new(out_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out_path, picture.data()).is_ok()
}

/// JSON mínimo con formato de audio para la ficha de calidad.
pub fn probe(path: &str) -> String {
    let mut file = match File::open(Path::new(path)) {
        Ok(f) => f,
        Err(_) => return "{}".to_string(),
    };
    let tagged = match lofty::read_from(&mut file) {
        Ok(t) => t,
        Err(_) => return "{}".to_string(),
    };
    let p = tagged.properties();
    format!(
        "{{\"durationMs\":{},\"sampleRate\":{},\"channels\":{},\"bitDepth\":{},\"bitrate\":{}}}",
        p.duration().as_millis() as u64,
        p.sample_rate().unwrap_or(0),
        p.channels().unwrap_or(0),
        p.bit_depth().unwrap_or(0),
        p.audio_bitrate().unwrap_or(0)
    )
}
