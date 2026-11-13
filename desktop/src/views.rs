use gpui::prelude::*;
use gpui::*;

use flacplayer::engine::Cmd;
use flacplayer::lyrics::LyricSource;
use flacplayer::shared::{STATUS_ENDED, STATUS_PAUSED, STATUS_PLAYING};

use crate::lyrics::{LyricsMode, LyricsStatus};
use crate::theme::*;
use crate::{Drill, FlacBox, Tab};

const SEEK_STEPS: usize = 120;

fn icon(s: &'static str, size: f32, color: u32) -> impl IntoElement {
    div().text_size(px(size)).text_color(rgb(color)).child(s)
}

fn round_btn(
    id: &'static str,
    glyph: &'static str,
    active: bool,
    cx: &mut Context<FlacBox>,
    f: impl Fn(&mut FlacBox) + 'static + Clone,
) -> impl IntoElement {
    let color = if active { ACCENT } else { MUTED };
    div()
        .id(id)
        .size(px(34.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
        .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
            f(this);
            cx.notify();
        }))
        .child(icon(glyph, 15., color))
}

// ------------------------------------------------------------- lateral

pub fn sidebar(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let tab_item = |id: &'static str, label: &'static str, tab: Tab, cx: &mut Context<FlacBox>| {
        let active = this.tab == tab && this.drill.is_none();
        let color = if active { ACCENT } else { TEXT };
        div()
            .id(id)
            .h(px(38.))
            .px_3()
            .rounded(px(8.))
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .when(active, |el| el.bg(rgb(SURFACE_HI)))
            .hover(|s| s.bg(rgb(SURFACE_HOVER)))
            .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                this.set_tab(tab);
                cx.notify();
            }))
            .child(icon(
                match tab {
                    Tab::Songs => "♫",
                    Tab::Albums => "▤",
                    Tab::Artists => "☰",
                },
                14.,
                color,
            ))
            .child(
                div()
                    .text_size(px(13.5))
                    .text_color(rgb(color))
                    .child(label),
            )
    };

    let songs = tab_item("nav-songs", "Canciones", Tab::Songs, cx);
    let albums = tab_item("nav-albums", "Álbumes", Tab::Albums, cx);
    let artists = tab_item("nav-artists", "Artistas", Tab::Artists, cx);

    let folder_rows = this
        .folders
        .iter()
        .map(|f| {
            let name = f
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| f.to_string_lossy().to_string());
            div()
                .px_3()
                .py_1()
                .text_size(px(11.5))
                .text_color(rgb(MUTED))
                .child(truncate(&name, 26))
        })
        .collect::<Vec<_>>();
    let playlist_rows = this
        .playlists
        .iter()
        .map(|playlist| {
            div()
                .px_3()
                .py_1()
                .text_size(px(11.5))
                .text_color(rgb(MUTED))
                .child(truncate(&playlist.name, 26))
        })
        .collect::<Vec<_>>();

    div()
        .w(px(216.))
        .h_full()
        .flex()
        .flex_col()
        .bg(rgb(SURFACE))
        .border_r_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .px_4()
                .pt_5()
                .pb_4()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(20.))
                        .font_weight(FontWeight::BOLD)
                        .child("FlacBox"),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(MUTED))
                        .child("FLAC · Linux desktop"),
                ),
        )
        .child(
            div()
                .px_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(songs)
                .child(albums)
                .child(artists),
        )
        .child(div().flex_1())
        .child(
            div()
                .px_2()
                .pb_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .px_3()
                        .text_size(px(10.5))
                        .text_color(rgb(MUTED))
                        .child("PLAYLISTS"),
                )
                .children(playlist_rows)
                .child(
                    div()
                        .id("create-playlist")
                        .px_3()
                        .h(px(30.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                        .on_click(cx.listener(|this, _ev: &ClickEvent, window, cx| {
                            this.playlist_modal_open = true;
                            window.focus(&this.playlist_focus);
                            cx.notify();
                        }))
                        .child(icon("＋", 13., ACCENT))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(rgb(ACCENT))
                                .child("Crear playlist"),
                        ),
                )
                .child(
                    div()
                        .px_3()
                        .text_size(px(10.5))
                        .text_color(rgb(MUTED))
                        .child("CARPETAS"),
                )
                .children(folder_rows)
                .child(
                    div()
                        .id("add-folder")
                        .px_3()
                        .h(px(30.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                        .on_click(cx.listener(|this, _ev: &ClickEvent, window, cx| {
                            this.add_folder_open = true;
                            window.focus(&this.folder_focus);
                            cx.notify();
                        }))
                        .child(icon("＋", 13., ACCENT))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(rgb(ACCENT))
                                .child("Añadir carpeta"),
                        ),
                ),
        )
        .child(
            div()
                .px_4()
                .py_3()
                .text_size(px(10.5))
                .text_color(rgb(MUTED))
                .child(if this.lib.scanning {
                    "escaneando…".to_string()
                } else {
                    format!("{} pistas", this.lib.tracks.len())
                }),
        )
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let cut: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    } else {
        s.to_string()
    }
}

// ------------------------------------------------------------- principal

pub fn main_area(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let (title, subtitle) = if let Some(drill) = &this.drill {
        (drill.title.clone(), drill.subtitle.clone())
    } else {
        match this.tab {
            Tab::Songs => (
                "Canciones".to_string(),
                format!("{} pistas", this.lib.tracks.len()),
            ),
            Tab::Albums => (
                "Álbumes".to_string(),
                format!("{} álbumes", this.lib.albums.len()),
            ),
            Tab::Artists => (
                "Artistas".to_string(),
                format!("{} artistas", this.lib.artists.len()),
            ),
        }
    };

    let total_ms: i64 = this
        .drill
        .as_ref()
        .map(|d| {
            d.tracks
                .iter()
                .map(|&i| this.lib.tracks[i].duration_ms as i64)
                .sum()
        })
        .unwrap_or(0);
    let subtitle = if total_ms > 0 {
        format!("{} · {}", subtitle, duration_long(total_ms))
    } else {
        subtitle
    };

    let search = div()
        .id("search-box")
        .w(px(260.))
        .h(px(34.))
        .px_3()
        .rounded(px(8.))
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(BORDER))
        .flex()
        .items_center()
        .gap_2()
        .track_focus(&this.search_focus)
        .cursor_text()
        .on_click(cx.listener(|this, _ev: &ClickEvent, window, cx| {
            window.focus(&this.search_focus);
            cx.notify();
        }))
        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _window, cx| {
            match ev.keystroke.key.as_str() {
                "backspace" => {
                    this.search.pop();
                    this.invalidate_search();
                }
                "escape" => this.clear_search(),
                _ => {
                    if let Some(ch) = &ev.keystroke.key_char {
                        if !ch.chars().any(|c| c.is_control()) {
                            this.search.push_str(ch);
                            this.invalidate_search();
                        }
                    }
                }
            }
            cx.notify();
        }))
        .child(icon("⌕", 14., MUTED))
        .child(
            div()
                .flex_1()
                .text_size(px(12.5))
                .text_color(rgb(if this.search.is_empty() { MUTED } else { TEXT }))
                .child(if this.search.is_empty() {
                    "Buscar en la biblioteca…".to_string()
                } else {
                    this.search.clone()
                }),
        );

    let back: AnyElement = if this.drill.is_some() {
        div()
            .id("back-drill")
            .h(px(28.))
            .px_2()
            .mb_1()
            .rounded(px(5.))
            .flex()
            .items_center()
            .gap_1()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(SURFACE_HOVER)))
            .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                this.clear_drill();
                cx.notify();
            }))
            .child(icon("‹", 14., MUTED))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child("Atrás"),
            )
            .into_any_element()
    } else {
        div().into_any_element()
    };

    let header = div()
        .h(px(84.))
        .px_6()
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .flex()
                .flex_col()
                .child(back)
                .child(
                    div()
                        .text_size(px(19.))
                        .font_weight(FontWeight::BOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(MUTED))
                        .child(subtitle),
                ),
        )
        .child(
            div().flex().items_center().gap_2().child(search).child(
                div()
                    .id("rescan")
                    .size(px(34.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(SURFACE))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                    .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| this.rescan(cx)))
                    .child(icon("⟳", 14., MUTED)),
            ),
        );

    let content: AnyElement = match this.tab {
        _ if this.drill.is_some() => songs_list(this, cx).into_any_element(),
        Tab::Songs => songs_list(this, cx).into_any_element(),
        Tab::Albums => albums_grid(this, cx).into_any_element(),
        Tab::Artists => artists_list(this, cx).into_any_element(),
    };

    div()
        .flex()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex_col()
        .child(header)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .px_4()
                .pt_2()
                .child(content),
        )
}

fn duration_long(ms: i64) -> String {
    let total = (ms / 1000) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    if h > 0 {
        format!("{h} h {m} min")
    } else {
        format!("{m} min")
    }
}

pub fn songs_list(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> AnyElement {
    let list = this.visible_tracks();

    if list.is_empty() {
        return empty_state(if this.lib.scanning {
            "Escaneando la biblioteca…"
        } else if !this.search.is_empty() {
            "Sin resultados para la búsqueda"
        } else {
            "No hay música todavía. Añade una carpeta."
        })
        .into_any_element();
    }

    let rows = uniform_list(
        "track-list",
        list.len(),
        cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
            range
                .map(|pos| track_row(this, list.as_slice(), pos, cx))
                .collect()
        }),
    )
    .flex_1()
    .min_h_0();

    div()
        .size_full()
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(30.))
                .px_3()
                .flex()
                .items_center()
                .gap_3()
                .text_size(px(10.))
                .text_color(rgb(MUTED))
                .child(div().w(px(20.)).child("#"))
                .child(div().w(px(170.)).child("TÍTULO"))
                .child(div().flex_1().child("ARTISTA"))
                .child(div().w(px(200.)).child("ÁLBUM"))
                .child(div().w(px(86.)).child("CALIDAD"))
                .child(div().w(px(48.)).child("DURACIÓN"))
                .child(div().w(px(28.)).child("")),
        )
        .child(rows)
        .into_any_element()
}

fn track_row(
    this: &mut FlacBox,
    list: &[usize],
    pos: usize,
    cx: &mut Context<FlacBox>,
) -> impl IntoElement {
    let index = list[pos];
    let track = this.lib.tracks[index].clone();
    let is_current = this
        .snap
        .index
        .ge(&0)
        .then(|| this.queue.get(this.snap.index as usize).cloned())
        .flatten()
        .map(|p| p == track.path)
        .unwrap_or(false);
    let playing = is_current && this.snap.status == STATUS_PLAYING;

    let list_owned = list.to_vec();
    let click_list = list_owned.clone();
    let playlist_path = track.path.clone();
    let playlist_menu_open = this.playlist_menu.as_ref() == Some(&playlist_path);
    let playlist_items = this
        .playlists
        .iter()
        .map(|playlist| {
            let name = playlist.name.clone();
            let label = name.clone();
            let path = playlist_path.clone();
            div()
                .id(("playlist-item", pos))
                .h(px(28.))
                .px_2()
                .rounded(px(5.))
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                    this.add_to_playlist(&name, path.clone());
                    cx.notify();
                }))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT))
                        .child(truncate(&label, 24)),
                )
        })
        .collect::<Vec<_>>();

    div()
        .id(("row", pos))
        .h(px(42.))
        .px_3()
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap_3()
        .cursor_pointer()
        .when(is_current, |el| el.bg(rgb(SURFACE_HI)))
        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
        .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
            this.play_tracks(&click_list, pos);
            cx.notify();
        }))
        .child(
            div()
                .w(px(20.))
                .text_size(px(11.))
                .text_color(rgb(if is_current { ACCENT } else { MUTED }))
                .child(if playing {
                    "▶".to_string()
                } else {
                    format!("{}", pos + 1)
                }),
        )
        .child(
            div()
                .w(px(170.))
                .text_size(px(12.5))
                .text_color(rgb(if is_current { ACCENT } else { TEXT }))
                .child(truncate(&track.title, 28)),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(12.))
                .text_color(rgb(MUTED))
                .child(truncate(&track.artist, 34)),
        )
        .child(
            div()
                .w(px(200.))
                .text_size(px(11.5))
                .text_color(rgb(MUTED))
                .child(truncate(&track.album, 30)),
        )
        .child(
            div()
                .w(px(86.))
                .text_size(px(10.))
                .text_color(rgb(ACCENT_DIM))
                .child(track.quality()),
        )
        .child(
            div()
                .w(px(48.))
                .text_size(px(11.))
                .text_color(rgb(MUTED))
                .child(fmt_time(track.duration_ms as i64)),
        )
        .child(
            div()
                .relative()
                .w(px(28.))
                .h(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .id(("track-menu", pos))
                        .size(px(26.))
                        .rounded(px(5.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                        .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                            this.playlist_menu =
                                if this.playlist_menu.as_ref() == Some(&playlist_path) {
                                    None
                                } else {
                                    Some(playlist_path.clone())
                                };
                            cx.notify();
                        }))
                        .child(div().text_size(px(15.)).text_color(rgb(MUTED)).child("⋯")),
                )
                .when(playlist_menu_open, |el| {
                    el.child(
                        div()
                            .absolute()
                            .right(px(30.))
                            .top(px(4.))
                            .w(px(190.))
                            .p_2()
                            .rounded(px(8.))
                            .bg(rgb(SURFACE))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .text_size(px(10.))
                                    .text_color(rgb(MUTED))
                                    .child("Añadir a playlist"),
                            )
                            .children(playlist_items),
                    )
                }),
        )
}

fn empty_state(text: &'static str) -> impl IntoElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(13.))
        .text_color(rgb(MUTED))
        .child(text)
}

fn albums_grid(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    if this.lib.albums.is_empty() {
        return empty_state("No hay álbumes todavía.").into_any_element();
    }

    let albums = this.lib.albums.clone();
    const COLUMNS: usize = 5;
    let rows = albums.len().div_ceil(COLUMNS);

    uniform_list(
        "albums-list",
        rows,
        cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
            range
                .map(|row| {
                    let start = row * COLUMNS;
                    let end = (start + COLUMNS).min(albums.len());
                    let cards = (start..end)
                        .map(|index| album_card(this, &albums[index], index, cx))
                        .collect::<Vec<_>>();
                    div().w_full().flex().gap_5().children(cards)
                })
                .collect()
        }),
    )
    .size_full()
    .pt_3()
    .into_any_element()
}

fn album_card(
    this: &mut FlacBox,
    album: &crate::library::Album,
    index: usize,
    cx: &mut Context<FlacBox>,
) -> AnyElement {
    let first_path = album
        .tracks
        .first()
        .map(|&track| this.lib.tracks[track].path.clone());
    let art = first_path.and_then(|path| this.art(&path));
    let cover: AnyElement = match art {
        Some(image) => img(image)
            .w(px(176.))
            .h(px(176.))
            .rounded(px(8.))
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
        None => div()
            .w(px(176.))
            .h(px(176.))
            .rounded(px(8.))
            .bg(rgb(SURFACE_HI))
            .flex()
            .items_center()
            .justify_center()
            .child(icon("♫", 26., ACCENT_DIM))
            .into_any_element(),
    };
    let tracks = album.tracks.clone();
    let name = album.name.clone();
    let artist = album.artist.clone();
    let year = album.year.clone();
    let track_count = album.tracks.len();

    div()
        .id(("album", index))
        .w(px(176.))
        .mb_5()
        .flex()
        .flex_col()
        .gap_2()
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
            this.open_drill(Drill {
                title: name.clone(),
                subtitle: artist.clone(),
                tracks: tracks.clone(),
            });
            this.tab = Tab::Albums;
            cx.notify();
        }))
        .child(cover)
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgb(TEXT))
                .child(truncate(&album.name, 22)),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(MUTED))
                .child(truncate(&album.artist, 22)),
        )
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(ACCENT_DIM))
                .child(format!(
                    "{} pistas{}",
                    track_count,
                    year.as_deref()
                        .map(|value| format!(" · {value}"))
                        .unwrap_or_default()
                )),
        )
        .into_any_element()
}

fn artists_list(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    if this.lib.artists.is_empty() {
        return empty_state("No hay artistas todavía.").into_any_element();
    }

    let artists = this.lib.artists.clone();
    let rows = artists
        .iter()
        .enumerate()
        .map(|(i, artist)| {
            let tracks = artist.tracks.clone();
            let name = artist.name.clone();
            let click_tracks = tracks.clone();
            let click_name = name.clone();
            let albums = artist.albums;
            div()
                .id(("artist", i))
                .h(px(46.))
                .px_3()
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap_3()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                    this.open_drill(Drill {
                        title: click_name.clone(),
                        subtitle: format!("{} álbumes", albums),
                        tracks: click_tracks.clone(),
                    });
                    this.tab = Tab::Artists;
                    cx.notify();
                }))
                .child(icon("☰", 12., MUTED))
                .child(div().flex_1().text_size(px(13.)).child(name))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(format!("{albums} álbumes")),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(format!("{} pistas", tracks_len(&tracks))),
                )
        })
        .collect::<Vec<_>>();

    div()
        .id("artists-scroll")
        .size_full()
        .overflow_y_scroll()
        .children(rows)
        .into_any_element()
}

fn tracks_len(tracks: &[usize]) -> usize {
    tracks.len()
}

// ------------------------------------------------------------- barra inferior

pub fn player_bar(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let current = this.current_track().cloned();
    let art = current.as_ref().and_then(|t| this.art(&t.path));

    let cover: AnyElement = match art {
        Some(image) => img(image)
            .size(px(52.))
            .rounded(px(6.))
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
        None => div()
            .size(px(52.))
            .rounded(px(6.))
            .bg(rgb(SURFACE_HI))
            .flex()
            .items_center()
            .justify_center()
            .child(icon("♫", 20., ACCENT_DIM))
            .into_any_element(),
    };

    let (title, artist) = match &current {
        Some(t) => (t.title.clone(), format!("{} · {}", t.artist, t.album)),
        None => ("Nada sonando".to_string(), "Elige una pista".to_string()),
    };

    let playing = this.snap.status == STATUS_PLAYING;
    let (status_text, status_color) = match this.snap.status {
        STATUS_PLAYING => ("REPRODUCIENDO".to_string(), ACCENT),
        STATUS_PAUSED => ("EN PAUSA".to_string(), MUTED),
        STATUS_ENDED => ("FIN DE LA COLA".to_string(), MUTED),
        _ => {
            if let Some(err) = &this.status_line {
                (format!("ERROR: {err}"), 0xE08A8A)
            } else {
                ("DETENIDO".to_string(), MUTED)
            }
        }
    };

    let position = this.snap.position_ms;
    let duration = this.snap.duration_ms;

    div()
        .h(px(90.))
        .w_full()
        .px_4()
        .flex()
        .items_center()
        .gap_4()
        .bg(rgb(SURFACE))
        .border_t_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .id("cover-btn")
                .cursor_pointer()
                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                    this.show_player = true;
                    cx.notify();
                }))
                .child(cover),
        )
        .child(
            div()
                .w(px(220.))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .child(truncate(&title, 30)),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(truncate(&artist, 32)),
                )
                .child(
                    div()
                        .text_size(px(9.))
                        .text_color(rgb(status_color))
                        .child(status_text),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(round_btn("shuffle", "⇄", this.shuffle, cx, |this| {
                    this.shuffle = !this.shuffle;
                    this.player.send(Cmd::SetShuffle(this.shuffle));
                }))
                .child(
                    div()
                        .id("prev")
                        .size(px(34.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                        .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                            this.player.send(Cmd::Prev);
                            cx.notify();
                        }))
                        .child(icon("⏮", 15., TEXT)),
                )
                .child(
                    div()
                        .id("play")
                        .size(px(42.))
                        .rounded_full()
                        .bg(rgb(ACCENT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.9))
                        .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                            this.toggle_play();
                            cx.notify();
                        }))
                        .child(icon(if playing { "▮▮" } else { "▶" }, 16., BG)),
                )
                .child(
                    div()
                        .id("next")
                        .size(px(34.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                        .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                            this.player.send(Cmd::Next);
                            cx.notify();
                        }))
                        .child(icon("⏭", 15., TEXT)),
                )
                .child(round_btn("repeat", "↻", this.repeat > 0, cx, |this| {
                    this.repeat = (this.repeat + 1) % 3;
                    this.player.send(Cmd::SetRepeat(this.repeat));
                })),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(44.))
                        .text_size(px(10.5))
                        .text_color(rgb(MUTED))
                        .child(fmt_time(position)),
                )
                .child(seek_bar(this, position, duration, cx))
                .child(
                    div()
                        .w(px(44.))
                        .text_size(px(10.5))
                        .text_color(rgb(MUTED))
                        .child(fmt_time(duration)),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id("mute-btn")
                        .size(px(30.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                        .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                            this.toggle_mute();
                            cx.notify();
                        }))
                        .child(icon(
                            volume_icon(this),
                            14.,
                            if this.muted { MUTED } else { TEXT },
                        )),
                )
                .child(volume_bar(this, cx))
                .child(
                    div()
                        .id("queue-btn")
                        .h(px(30.))
                        .px_3()
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .cursor_pointer()
                        .when(this.show_queue, |el| el.bg(rgb(SURFACE_HI)))
                        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                        .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                            this.show_queue = !this.show_queue;
                            cx.notify();
                        }))
                        .child(icon("☰", 12., if this.show_queue { ACCENT } else { MUTED }))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(if this.show_queue { ACCENT } else { MUTED }))
                                .child("Cola"),
                        ),
                )
                .child(
                    div()
                        .id("expand-btn")
                        .size(px(32.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                        .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                            this.show_player = true;
                            cx.notify();
                        }))
                        .child(icon("⤢", 14., MUTED)),
                ),
        )
}

fn seek_bar(
    this: &mut FlacBox,
    position: i64,
    duration: i64,
    cx: &mut Context<FlacBox>,
) -> impl IntoElement {
    let frac = if duration > 0 {
        (position as f32 / duration as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = (frac * SEEK_STEPS as f32).round() as usize;

    let mut segments = Vec::with_capacity(SEEK_STEPS);
    for i in 0..SEEK_STEPS {
        let frac_step = i as f32 / (SEEK_STEPS - 1) as f32;
        let color = if i <= filled { ACCENT } else { SURFACE_HI };
        segments.push(
            div()
                .id(("seek", i))
                .flex_1()
                .h_full()
                .bg(rgb(color))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                    let dur = this.snap.duration_ms;
                    if dur > 0 {
                        this.seek_to((dur as f32 * frac_step) as i64);
                        cx.notify();
                    }
                })),
        );
    }

    div().flex_1().h(px(18.)).flex().items_center().child(
        div()
            .w_full()
            .h(px(6.))
            .rounded_full()
            .overflow_hidden()
            .flex()
            .children(segments),
    )
}

fn volume_icon(this: &FlacBox) -> &'static str {
    if this.muted || this.volume <= 0.0 {
        "×"
    } else if this.volume < 0.5 {
        "◒"
    } else {
        "◉"
    }
}

fn volume_bar(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    const STEPS: usize = 10;
    let filled = (this.volume * STEPS as f32).round() as usize;
    let segments = (0..STEPS)
        .map(|index| {
            let value = (index + 1) as f32 / STEPS as f32;
            div()
                .id(("volume", index))
                .w(px(7.))
                .h(px(14.))
                .rounded(px(2.))
                .bg(rgb(if index < filled { ACCENT } else { SURFACE_HI }))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                    this.set_volume(value);
                    cx.notify();
                }))
        })
        .collect::<Vec<_>>();

    div()
        .w(px(84.))
        .h(px(24.))
        .flex()
        .items_center()
        .gap(px(2.))
        .children(segments)
}

// ------------------------------------------------------------- now playing

pub fn now_playing(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> AnyElement {
    let current = this.current_track().cloned();
    let art = current.as_ref().and_then(|t| this.art(&t.path));

    let cover: AnyElement = match art {
        Some(image) => img(image)
            .w(px(340.))
            .h(px(340.))
            .rounded(px(12.))
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
        None => div()
            .size(px(340.))
            .rounded(px(12.))
            .bg(rgb(SURFACE_HI))
            .flex()
            .items_center()
            .justify_center()
            .child(icon("♫", 60., ACCENT_DIM))
            .into_any_element(),
    };

    let (title, artist, album, quality) = match &current {
        Some(t) => (
            t.title.clone(),
            t.artist.clone(),
            t.album.clone(),
            t.quality(),
        ),
        None => (
            "Nada sonando".to_string(),
            "Elige una pista".to_string(),
            String::new(),
            String::new(),
        ),
    };

    let position = this.snap.position_ms;
    let duration = this.snap.duration_ms;
    let playing = this.snap.status == STATUS_PLAYING;

    let queue_rows = this
        .queue
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let is_current = i as i32 == this.snap.index;
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            div()
                .id(("queue", i))
                .h(px(32.))
                .px_2()
                .rounded(px(5.))
                .flex()
                .items_center()
                .gap_2()
                .when(is_current, |el| el.bg(rgb(SURFACE_HI)))
                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                .child(
                    div()
                        .w(px(18.))
                        .text_size(px(10.))
                        .text_color(rgb(if is_current { ACCENT } else { MUTED }))
                        .child(if is_current {
                            "▶".to_string()
                        } else {
                            format!("{}", i + 1)
                        }),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(if is_current { ACCENT } else { TEXT }))
                        .child(truncate(&name, 26)),
                )
        })
        .collect::<Vec<_>>();

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap_6()
        .p_6()
        .bg(rgb(BG))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_4()
                .child(
                    div()
                        .id("back-btn")
                        .h(px(30.))
                        .px_3()
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                        .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                            this.show_player = false;
                            cx.notify();
                        }))
                        .child(icon("⌄", 14., MUTED))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(MUTED))
                                .child("Biblioteca"),
                        ),
                )
                .child(cover)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .max_w(px(520.))
                        .child(
                            div()
                                .text_size(px(24.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(TEXT))
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(rgb(MUTED))
                                .child(format!("{artist} · {album}")),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(ACCENT_DIM))
                                .child(quality),
                        ),
                )
                .child(
                    div()
                        .w(px(420.))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(seek_bar(this, position, duration, cx))
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .text_size(px(10.5))
                                .text_color(rgb(MUTED))
                                .child(fmt_time(position))
                                .child(fmt_time(duration)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(round_btn("np-shuffle", "⇄", this.shuffle, cx, |this| {
                            this.shuffle = !this.shuffle;
                            this.player.send(Cmd::SetShuffle(this.shuffle));
                        }))
                        .child(
                            div()
                                .id("np-prev")
                                .size(px(40.))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                                    this.player.send(Cmd::Prev);
                                    cx.notify();
                                }))
                                .child(icon("⏮", 18., TEXT)),
                        )
                        .child(
                            div()
                                .id("np-play")
                                .size(px(56.))
                                .rounded_full()
                                .bg(rgb(ACCENT))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(|s| s.opacity(0.9))
                                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                                    this.toggle_play();
                                    cx.notify();
                                }))
                                .child(icon(if playing { "▮▮" } else { "▶" }, 20., BG)),
                        )
                        .child(
                            div()
                                .id("np-next")
                                .size(px(40.))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                                    this.player.send(Cmd::Next);
                                    cx.notify();
                                }))
                                .child(icon("⏭", 18., TEXT)),
                        )
                        .child(round_btn("np-repeat", "↻", this.repeat > 0, cx, |this| {
                            this.repeat = (this.repeat + 1) % 3;
                            this.player.send(Cmd::SetRepeat(this.repeat));
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .id("np-mute-btn")
                                        .size(px(32.))
                                        .rounded(px(6.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                                        .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                                            this.toggle_mute();
                                            cx.notify();
                                        }))
                                        .child(icon(
                                            volume_icon(this),
                                            15.,
                                            if this.muted { MUTED } else { TEXT },
                                        )),
                                )
                                .child(volume_bar(this, cx)),
                        )
                        .child(
                            div()
                                .id("np-lyrics-toggle")
                                .h(px(32.))
                                .px_3()
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap_2()
                                .cursor_pointer()
                                .when(this.show_lyrics, |el| el.bg(rgb(ACCENT_DIM)))
                                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                                    this.show_lyrics = !this.show_lyrics;
                                    cx.notify();
                                }))
                                .child(icon(
                                    "♫",
                                    15.,
                                    if this.show_lyrics { ACCENT } else { MUTED },
                                ))
                                .child(
                                    div()
                                        .text_size(px(10.5))
                                        .text_color(rgb(if this.show_lyrics {
                                            ACCENT
                                        } else {
                                            MUTED
                                        }))
                                        .child(if this.show_lyrics {
                                            "Ocultar letras"
                                        } else {
                                            "Mostrar letras"
                                        }),
                                ),
                        ),
                ),
        )
        .when(this.show_queue, |el| {
            el.child(
                div()
                    .w(px(300.))
                    .h_full()
                    .bg(rgb(SURFACE))
                    .border_l_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px_4()
                            .py_3()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(format!("COLA · {} pistas", this.queue.len())),
                    )
                    .child(
                        div()
                            .id("queue-scroll")
                            .flex_1()
                            .overflow_y_scroll()
                            .px_2()
                            .children(queue_rows),
                    ),
            )
        })
        .when(this.show_lyrics, |el| el.child(lyrics_panel(this, cx)))
        .into_any_element()
}

// ------------------------------------------------------------- letra

/// Recuadro de letra sincronizada, a la derecha de la vista "Ahora suena".
pub fn lyrics_panel(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let track_title = this
        .current_track()
        .map(|track| truncate(&track.title, 30))
        .unwrap_or_else(|| "Nada sonando".to_string());
    let badge = match this.lyrics.as_ref().map(|l| l.source) {
        Some(LyricSource::Embedded) => "embebida",
        Some(LyricSource::Sidecar) => "archivo .lrc",
        Some(LyricSource::Lrclib) => "lrclib.net",
        Some(LyricSource::Plain) => "sin sincronizar",
        Some(LyricSource::Unknown) | None => "",
    };

    let lines: Vec<AnyElement> = match this.lyrics.as_ref() {
        Some(lyrics) if !lyrics.is_empty() => {
            let active = this.lyrics_active.unwrap_or(0);
            let window = 3usize;
            let start = active.saturating_sub(window);
            let end = (active + window + 1).min(lyrics.lines.len());
            lyrics
                .lines
                .iter()
                .enumerate()
                .skip(start)
                .take(end - start)
                .map(|(i, line)| {
                    let is_active = lyrics.synced && this.lyrics_active == Some(i);
                    let distance = this
                        .lyrics_active
                        .map(|active| active.abs_diff(i))
                        .unwrap_or(99);
                    let color = if is_active {
                        ACCENT
                    } else if distance <= 1 {
                        TEXT
                    } else {
                        MUTED
                    };
                    div()
                        .id(("lyric", i))
                        .w_full()
                        .px_3()
                        .py_3()
                        .rounded(px(6.))
                        .when(is_active, |el| el.bg(rgb(SURFACE_HI)))
                        .text_size(px(if is_active {
                            18.
                        } else if distance <= 1 {
                            15.
                        } else {
                            13.5
                        }))
                        .text_color(rgb(color))
                        .when(!is_active, |el| {
                            el.opacity(if distance <= 1 { 0.85 } else { 0.58 })
                        })
                        .when(is_active, |el| el.font_weight(FontWeight::SEMIBOLD))
                        .child(if line.text.trim().is_empty() {
                            "·".to_string()
                        } else {
                            line.text.clone()
                        })
                        .into_any_element()
                })
                .collect()
        }
        _ => {
            let message = match this.lyrics_status {
                LyricsStatus::Loading => "Buscando letra…".to_string(),
                LyricsStatus::Empty => "Sin letra disponible".to_string(),
                _ => "Nada sonando".to_string(),
            };
            vec![div()
                .text_size(px(12.))
                .text_color(rgb(MUTED))
                .child(message)
                .into_any_element()]
        }
    };

    div()
        .flex_1()
        .min_w_0()
        .max_w(px(400.))
        .h(px(560.))
        .my_6()
        .flex()
        .flex_col()
        .rounded(px(14.))
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .h(px(92.))
                .flex_shrink_0()
                .px_4()
                .flex()
                .flex_col()
                .items_start()
                .justify_center()
                .gap_2()
                .border_b_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(rgb(ACCENT))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("LETRAS"),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(TEXT))
                                        .child(track_title),
                                ),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_full()
                                .bg(rgb(ACCENT_DIM))
                                .text_size(px(10.))
                                .text_color(rgb(ACCENT))
                                .child(badge),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(lyrics_source_button(LyricsMode::Auto, "Auto", this, cx))
                        .child(lyrics_source_button(
                            LyricsMode::Embedded,
                            "Embebida",
                            this,
                            cx,
                        ))
                        .child(lyrics_source_button(LyricsMode::Sidecar, ".LRC", this, cx))
                        .child(lyrics_source_button(LyricsMode::Cache, "Cache", this, cx))
                        .child(lyrics_source_button(LyricsMode::Online, "Online", this, cx)),
                ),
        )
        .child(
            div()
                .id("lyrics-scroll")
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .justify_center()
                .overflow_hidden()
                .px_5()
                .py_5()
                .children(lines),
        )
        .when(this.lyrics_status == LyricsStatus::Loading, |el| {
            el.opacity(0.6)
        })
}

fn lyrics_source_button(
    mode: LyricsMode,
    label: &'static str,
    this: &mut FlacBox,
    cx: &mut Context<FlacBox>,
) -> impl IntoElement {
    let active = this.lyrics_mode == mode;
    div()
        .id(label)
        .h(px(24.))
        .px_2()
        .rounded(px(5.))
        .flex()
        .items_center()
        .cursor_pointer()
        .when(active, |el| el.bg(rgb(ACCENT_DIM)))
        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
        .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
            this.lyrics_mode = mode;
            this.lyrics_track = None;
            cx.notify();
        }))
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(if active { ACCENT } else { MUTED }))
                .child(label),
        )
}

// ------------------------------------------------------------- modal playlist

pub fn create_playlist_modal(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let input_text = if this.playlist_input.is_empty() {
        "Nueva playlist".to_string()
    } else {
        this.playlist_input.clone()
    };
    let color = if this.playlist_input.is_empty() {
        MUTED
    } else {
        TEXT
    };

    div()
        .absolute()
        .inset_0()
        .bg(rgba(0x00000088))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(420.))
                .p_5()
                .rounded(px(12.))
                .bg(rgb(SURFACE))
                .border_1()
                .border_color(rgb(BORDER))
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Crear playlist"),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(MUTED))
                        .child("Dale un nombre para guardarla en tu biblioteca."),
                )
                .child(
                    div()
                        .id("playlist-input")
                        .h(px(36.))
                        .px_3()
                        .rounded(px(8.))
                        .bg(rgb(BG))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .flex()
                        .items_center()
                        .track_focus(&this.playlist_focus)
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _w, cx| {
                            match ev.keystroke.key.as_str() {
                                "backspace" => {
                                    this.playlist_input.pop();
                                }
                                "escape" => {
                                    this.playlist_modal_open = false;
                                    this.playlist_input.clear();
                                }
                                "enter" => this.create_playlist(),
                                _ => {
                                    if let Some(ch) = &ev.keystroke.key_char {
                                        if !ch.chars().any(|c| c.is_control()) {
                                            this.playlist_input.push_str(ch);
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(rgb(color))
                                .child(input_text),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            div()
                                .id("cancel-playlist")
                                .h(px(32.))
                                .px_4()
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                                    this.playlist_modal_open = false;
                                    this.playlist_input.clear();
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(MUTED))
                                        .child("Cancelar"),
                                ),
                        )
                        .child(
                            div()
                                .id("save-playlist")
                                .h(px(32.))
                                .px_4()
                                .rounded(px(6.))
                                .bg(rgb(ACCENT))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                                    this.create_playlist();
                                    cx.notify();
                                }))
                                .child(div().text_size(px(12.)).text_color(rgb(BG)).child("Crear")),
                        ),
                ),
        )
}

// ------------------------------------------------------------- modal carpeta

pub fn add_folder_modal(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let input_text = if this.folder_input.is_empty() {
        "~/Música".to_string()
    } else {
        this.folder_input.clone()
    };
    let color = if this.folder_input.is_empty() {
        MUTED
    } else {
        TEXT
    };

    div()
        .absolute()
        .inset_0()
        .bg(rgba(0x00000088))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(460.))
                .p_5()
                .rounded(px(12.))
                .bg(rgb(SURFACE))
                .border_1()
                .border_color(rgb(BORDER))
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Añadir carpeta"),
                )
                .child(
                    div().text_size(px(11.5)).text_color(rgb(MUTED)).child(
                        "Ruta absoluta con música (se guarda en ~/.config/flacbox/folders).",
                    ),
                )
                .child(
                    div()
                        .id("folder-input")
                        .h(px(36.))
                        .px_3()
                        .rounded(px(8.))
                        .bg(rgb(BG))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .flex()
                        .items_center()
                        .track_focus(&this.folder_focus)
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _w, cx| {
                            match ev.keystroke.key.as_str() {
                                "backspace" => {
                                    this.folder_input.pop();
                                }
                                "escape" => {
                                    this.add_folder_open = false;
                                    this.folder_input.clear();
                                }
                                "enter" => {
                                    let raw = expand_home(&this.folder_input);
                                    if !raw.is_empty() {
                                        this.add_folder(std::path::PathBuf::from(raw), cx);
                                    }
                                }
                                _ => {
                                    if let Some(ch) = &ev.keystroke.key_char {
                                        if !ch.chars().any(|c| c.is_control()) {
                                            this.folder_input.push_str(ch);
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(rgb(color))
                                .child(input_text),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            div()
                                .id("cancel-folder")
                                .h(px(32.))
                                .px_4()
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                                    this.add_folder_open = false;
                                    this.folder_input.clear();
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(MUTED))
                                        .child("Cancelar"),
                                ),
                        )
                        .child(
                            div()
                                .id("ok-folder")
                                .h(px(32.))
                                .px_4()
                                .rounded(px(6.))
                                .bg(rgb(ACCENT))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                                    let raw = expand_home(&this.folder_input);
                                    if !raw.is_empty() {
                                        this.add_folder(std::path::PathBuf::from(raw), cx);
                                    }
                                }))
                                .child(
                                    div().text_size(px(12.)).text_color(rgb(BG)).child("Añadir"),
                                ),
                        ),
                ),
        )
}

fn expand_home(input: &str) -> String {
    let input = input.trim();
    if let Some(rest) = input.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/{rest}")
    } else {
        input.to_string()
    }
}
