use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey};

/// Tags de un fichero de audio, ya interpretados.
#[derive(Clone, Debug, Default)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<String>,
    pub track: Option<String>,
    pub disc: Option<String>,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u8,
    pub bit_depth: Option<u8>,
    pub has_art: bool,
}

pub fn read(path: &Path) -> Option<Tags> {
    let tagged = Probe::open(path)
        .ok()?
        .options(ParseOptions::new().read_cover_art(false))
        .read()
        .ok()?;
    let props = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    Some(Tags {
        title: tag.and_then(|t| t.title()).map(|s| s.to_string()),
        artist: tag.and_then(|t| t.artist()).map(|s| s.to_string()),
        album: tag.and_then(|t| t.album()).map(|s| s.to_string()),
        album_artist: tag
            .and_then(|t| t.get_string(ItemKey::AlbumArtist))
            .map(|s| s.to_string()),
        genre: tag.and_then(|t| t.genre()).map(|s| s.to_string()),
        year: tag
            .and_then(|t| t.get_string(ItemKey::Year))
            .map(|s| s.to_string()),
        track: tag.and_then(|t| t.track()).map(|v| v.to_string()),
        disc: tag
            .and_then(|t| t.get_string(ItemKey::DiscNumber))
            .map(|s| s.to_string()),
        duration_ms: props.duration().as_millis() as u64,
        sample_rate: props.sample_rate().unwrap_or(0),
        channels: props.channels().unwrap_or(0),
        bit_depth: props.bit_depth(),
        has_art: tag.map(|t| !t.pictures().is_empty()).unwrap_or(false),
    })
}

/// Primera carátula incrustada, en bruto (jpg/png).
pub fn artwork(path: &Path) -> Option<Vec<u8>> {
    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let picture = tag.pictures().first()?;
    Some(picture.data().to_vec())
}
