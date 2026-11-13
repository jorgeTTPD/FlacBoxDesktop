use gpui::prelude::*;
use gpui::*;
use std::path::PathBuf;
use std::time::Duration;

use flacplayer::engine::Cmd;
use flacplayer::lyrics::LyricSource;
use flacplayer::shared::{STATUS_ENDED, STATUS_PAUSED, STATUS_PLAYING};

use crate::lyrics::{LyricsMode, LyricsStatus};
use crate::theme::*;
use crate::{Drill, FlacBox, NpMode, Tab};

fn icon(s: &'static str, size: f32, color: u32) -> impl IntoElement {
    div().text_size(px(size)).text_color(rgb(color)).child(s)
}

fn edit_text_field(field: &mut String, ev: &KeyDownEvent, cx: &mut Context<FlacBox>) -> bool {
    let key = ev.keystroke.key.as_str();
    let mods = &ev.keystroke.modifiers;
    let secondary = mods.control || mods.platform;
    if secondary && key == "v" {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let start = field.len();
            for ch in text.chars() {
                if !ch.is_control() {
                    field.push(ch);
                }
            }
            return field.len() != start;
        }
        return false;
    }
    if mods.control || mods.alt || mods.platform || mods.function {
        if mods.control && key == "backspace" {
            let mut changed = false;
            while field.chars().last().is_some_and(|c| c.is_whitespace()) {
                field.pop();
                changed = true;
            }
            while field.chars().last().is_some_and(|c| !c.is_whitespace()) {
                field.pop();
                changed = true;
            }
            return changed;
        }
        return false;
    }
    match key {
        "backspace" => field.pop().is_some(),
        _ => {
            if let Some(ch) = &ev.keystroke.key_char {
                if !ch.chars().any(|c| c.is_control()) {
                    field.push_str(ch);
                    return true;
                }
            }
            false
        }
    }
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

// ------------------------------------------------------------- repetir

fn repeat_button(
    id: &'static str,
    this: &mut FlacBox,
    cx: &mut Context<FlacBox>,
) -> impl IntoElement {
    let active = this.repeat > 0;
    let color = if active { ACCENT } else { MUTED };
    div()
        .id(id)
        .size(px(34.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
        .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
            this.repeat = (this.repeat + 1) % 3;
            this.player.send(Cmd::SetRepeat(this.repeat));
            cx.notify();
        }))
        .child(icon("↻", 15., color))
        .when(this.repeat == 2, |el| {
            el.child(
                div()
                    .absolute()
                    .bottom(px(4.))
                    .right(px(6.))
                    .text_size(px(9.))
                    .text_color(rgb(ACCENT))
                    .child("1"),
            )
        })
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
                    Tab::Internet => "◉",
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
    let internet = tab_item("nav-internet", "Internet", Tab::Internet, cx);

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
        .enumerate()
        .map(|(index, playlist)| {
            div()
                .id(("playlist", index))
                .px_3()
                .py_1()
                .rounded(px(6.))
                .text_size(px(11.5))
                .text_color(rgb(TEXT))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                    this.open_playlist(index);
                    cx.notify();
                }))
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
                .child(artists)
                .child(internet),
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

pub fn internet_view(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let searching = this.internet.searching;
    let error = this.internet.error.clone();
    let query = this.internet.query.clone();
    let result_count = this.internet.results.len();

    let (status_text, status_color) = if searching {
        ("Buscando en YouTube…".to_string(), ACCENT)
    } else if let Some(error) = &error {
        (truncate(error, 60).to_string(), 0xE08A8A)
    } else if query.is_empty() {
        ("YouTube vía yt-dlp".to_string(), MUTED)
    } else if result_count == 0 {
        (format!("Sin resultados para «{query}»"), MUTED)
    } else {
        (format!("{result_count} resultados"), MUTED)
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
                .child(
                    div()
                        .text_size(px(19.))
                        .font_weight(FontWeight::BOLD)
                        .child("Internet"),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(status_color))
                        .child(status_text),
                ),
        )
        .child(
            div()
                .id("internet-search-box")
                .w(px(340.))
                .h(px(36.))
                .flex_shrink_0()
                .px_3()
                .rounded(px(8.))
                .bg(rgb(SURFACE))
                .border_1()
                .border_color(rgb(if this.internet.query.is_empty() {
                    BORDER
                } else {
                    ACCENT_DIM
                }))
                .flex()
                .items_center()
                .gap_2()
                .track_focus(&this.internet.focus)
                .cursor_text()
                .on_click(cx.listener(|this, _ev: &ClickEvent, window, cx| {
                    window.focus(&this.internet.focus);
                    cx.notify();
                }))
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _window, cx| {
                    match ev.keystroke.key.as_str() {
                        "escape" => {
                            this.internet.query.clear();
                        }
                        "enter" => this.search_internet(),
                        _ => {
                            edit_text_field(&mut this.internet.query, ev, cx);
                        }
                    }
                    cx.notify();
                }))
                .child(icon("⌕", 15., MUTED))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(13.))
                        .text_color(rgb(if this.internet.query.is_empty() {
                            MUTED
                        } else {
                            TEXT
                        }))
                        .child(if this.internet.query.is_empty() {
                            "Buscar en YouTube…".to_string()
                        } else {
                            this.internet.query.clone()
                        }),
                )
                .child(
                    div()
                        .text_size(px(9.5))
                        .text_color(rgb(MUTED))
                        .child("ENTER"),
                ),
        );

    let body: AnyElement = if searching && this.internet.results.is_empty() {
        empty_state("Buscando en YouTube…").into_any_element()
    } else if let Some(error) = &this.internet.error {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(rgb(0xE08A8A))
                    .child("No se pudo completar la búsqueda"),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .max_w(px(520.))
                    .child(truncate(error, 120)),
            )
            .into_any_element()
    } else if this.internet.query.is_empty() {
        empty_state("Escribe algo y pulsa Enter — busca en YouTube").into_any_element()
    } else if this.internet.results.is_empty() {
        empty_state("Sin resultados").into_any_element()
    } else {
        uniform_list(
            "internet-results",
            this.internet.results.len(),
            cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                range.map(|index| internet_row(this, index, cx)).collect()
            }),
        )
        .size_full()
        .into_any_element()
    };

    div()
        .flex()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex_col()
        .child(header)
        .child(div().flex_1().min_w_0().min_h_0().px_4().pt_3().child(body))
}

fn internet_row(this: &mut FlacBox, index: usize, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let Some(result) = this.internet.results.get(index) else {
        return div().into_any_element();
    };
    let id = result.id.clone();
    let title = result.title.clone();
    let uploader = result.uploader.clone();
    let duration_ms = result.duration_ms;
    let view_count = result.view_count;
    let thumb = this.internet_thumb(&id);
    let art = this.art(&thumb);
    let is_downloading = this.internet.downloading.contains(&id);
    let is_downloaded = this.internet.downloaded.contains(&id);
    let is_video_downloading = this.internet.downloading_video.contains(&id);

    let cover: AnyElement = match art {
        Some(image) => img(image)
            .w(px(150.))
            .h(px(85.))
            .rounded(px(8.))
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
        None => div()
            .w(px(150.))
            .h(px(85.))
            .rounded(px(8.))
            .bg(rgb(SURFACE_HI))
            .flex()
            .items_center()
            .justify_center()
            .child(icon("◉", 22., ACCENT_DIM))
            .into_any_element(),
    };

    div()
        .id(("yt-row", index))
        .h(px(96.))
        .px_2()
        .rounded(px(8.))
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
        .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
            this.play_internet(index);
            cx.notify();
        }))
        .child(cover)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(rgb(TEXT))
                        .child(truncate(&title, 58)),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(truncate(&uploader, 48)),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(ACCENT_DIM))
                        .child(format!(
                            "{} · {}",
                            fmt_time(duration_ms as i64),
                            fmt_views(view_count)
                        )),
                ),
        )
        .child(
            div()
                .w(px(118.))
                .flex()
                .flex_col()
                .items_end()
                .gap_2()
                .child(
                    div()
                        .text_size(px(9.))
                        .text_color(rgb(if is_downloading || is_video_downloading {
                            ACCENT
                        } else {
                            MUTED
                        }))
                        .child(if is_video_downloading {
                            "DESCARGANDO VIDEO…"
                        } else if is_downloading {
                            "DESCARGANDO…"
                        } else if is_downloaded {
                            "EN CACHE"
                        } else {
                            "▷"
                        }),
                )
                .child(
                    div().flex().items_center().gap_1().child(
                        div()
                            .id(("video-row", index))
                            .size(px(32.))
                            .rounded(px(7.))
                            .bg(rgb(if is_video_downloading {
                                SURFACE_HOVER
                            } else {
                                SURFACE_HI
                            }))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                            .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                                cx.stop_propagation();
                                this.play_video(index);
                                cx.notify();
                            }))
                            .child(icon(
                                "▣",
                                14.,
                                if is_video_downloading { ACCENT } else { TEXT },
                            )),
                    ),
                ),
        )
        .into_any_element()
}

fn fmt_views(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{} M de vistas", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{} mil vistas", n / 1_000)
    } else if n > 0 {
        format!("{n} vistas")
    } else {
        "—".to_string()
    }
}

// ------------------------------------------------------------- principal

pub fn main_area(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> AnyElement {
    if this.tab == Tab::Internet {
        return internet_view(this, cx).into_any_element();
    }

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
            Tab::Internet => ("Internet".to_string(), "YouTube vía yt-dlp".to_string()),
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
                "escape" => this.clear_search(),
                _ => {
                    if edit_text_field(&mut this.search, ev, cx) {
                        this.invalidate_search();
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
        Tab::Internet => internet_view(this, cx).into_any_element(),
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
        .into_any_element()
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
                .child(div().w(px(60.)).child("DURACIÓN"))
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
                    cx.stop_propagation();
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
            this.playlist_menu = None;
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
                .w(px(60.))
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
                            cx.stop_propagation();
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

// ------------------------------------------------------------- video

fn video_slot_el(this: &FlacBox, size: f32) -> AnyElement {
    let Some(video) = &this.video.video else {
        return div().into_any_element();
    };
    let height = size * 0.5625;
    div()
        .w(px(size))
        .h(px(height))
        .rounded(px(10.))
        .overflow_hidden()
        .child(
            gpui_video_player::video(video.clone())
                .id("video-np")
                .buffer_capacity(15)
                .size(px(size), px(height)),
        )
        .into_any_element()
}

fn video_seek_track(
    duration: Duration,
    position: Duration,
    cx: &mut Context<FlacBox>,
) -> impl IntoElement {
    let frac = if duration.as_millis() > 0 {
        (position.as_millis() as f32 / duration.as_millis() as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    const STEPS: usize = 72;
    let filled = if frac > 0.0 {
        ((frac * STEPS as f32).round() as usize).max(1)
    } else {
        0
    };

    let mut segments = Vec::with_capacity(STEPS);
    for i in 0..STEPS {
        let frac_step = i as f32 / (STEPS - 1) as f32;
        let color = if i < filled { ACCENT } else { SURFACE_HI };
        segments.push(
            div()
                .id(("vseek", i))
                .flex_1()
                .h_full()
                .bg(rgb(color))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                    if let Some(video) = &this.video.video {
                        let seconds = duration.as_secs_f32() * frac_step;
                        let _ = video.seek(Duration::from_secs_f32(seconds), false);
                        cx.notify();
                    }
                })),
        );
    }

    div()
        .flex_1()
        .h(px(6.))
        .rounded_full()
        .overflow_hidden()
        .flex()
        .children(segments)
}

fn video_seek_row(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> AnyElement {
    let (position, duration) = match &this.video.video {
        Some(video) => (video.position(), video.duration()),
        None => (Duration::ZERO, Duration::ZERO),
    };
    let pos_ms = position.as_millis() as i64;
    let dur_ms = duration.as_millis() as i64;

    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(46.))
                .text_size(px(10.5))
                .text_color(rgb(MUTED))
                .child(fmt_time(pos_ms)),
        )
        .child(video_seek_track(duration, position, cx))
        .child(
            div()
                .w(px(46.))
                .flex()
                .justify_end()
                .text_size(px(10.5))
                .text_color(rgb(MUTED))
                .child(fmt_time(dur_ms)),
        )
        .into_any_element()
}

fn video_transport_row(
    this: &mut FlacBox,
    size: f32,
    cx: &mut Context<FlacBox>,
) -> impl IntoElement {
    let paused = this
        .video
        .video
        .as_ref()
        .map(|video| video.paused())
        .unwrap_or(true);
    div().flex().items_center().gap_5().child(circle_transport(
        "video-play",
        if paused { "▶" } else { "▮▮" },
        size,
        true,
        cx,
        |this| {
            if let Some(video) = &this.video.video {
                video.set_paused(!video.paused());
            }
        },
    ))
}

fn seek_area(
    this: &mut FlacBox,
    position: i64,
    duration: i64,
    thickness: f32,
    cx: &mut Context<FlacBox>,
) -> AnyElement {
    if this.video.active {
        return video_seek_row(this, cx);
    }
    np_seek_row(position, duration, thickness, cx).into_any_element()
}

fn transport_area(
    this: &mut FlacBox,
    playing: bool,
    size: f32,
    cx: &mut Context<FlacBox>,
) -> AnyElement {
    if this.video.active {
        return video_transport_row(this, size, cx).into_any_element();
    }
    transport_row(playing, size, cx).into_any_element()
}

// ------------------------------------------------------------- barra inferior

pub fn player_bar(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let current = this.np_track().cloned();
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
                .child(repeat_button("repeat", this, cx)),
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
                .child(seek_track(
                    position,
                    duration,
                    rgb(ACCENT),
                    rgb(SURFACE_HI),
                    6.,
                    cx,
                ))
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

// ------------------------------------------------------------- vista completa

fn np_gradient(color: Option<u32>, top_mix: f32, bottom_mix: f32) -> Background {
    let a = mix_rgb(color.unwrap_or(0x1A2A24), BG, top_mix);
    let b = mix_rgb(color.unwrap_or(0x0E1512), BG, bottom_mix);
    linear_gradient(
        168.,
        linear_color_stop(rgb(a), 0.0),
        linear_color_stop(rgb(b), 1.0),
    )
}

fn cover_el(this: &mut FlacBox, path: Option<&PathBuf>, size: f32, radius: f32) -> AnyElement {
    let art = path.and_then(|path| this.art(path));
    match art {
        Some(image) => img(image)
            .w(px(size))
            .h(px(size))
            .rounded(px(radius))
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
        None => div()
            .w(px(size))
            .h(px(size))
            .rounded(px(radius))
            .bg(rgb(SURFACE_HI))
            .flex()
            .items_center()
            .justify_center()
            .child(icon("♫", size * 0.32, ACCENT_DIM))
            .into_any_element(),
    }
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

fn seek_track(
    position: i64,
    duration: i64,
    fill: Rgba,
    track: Rgba,
    thickness: f32,
    cx: &mut Context<FlacBox>,
) -> impl IntoElement {
    let frac = if duration > 0 {
        (position as f32 / duration as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    const STEPS: usize = 72;
    let filled = if frac > 0.0 {
        ((frac * STEPS as f32).round() as usize).max(1)
    } else {
        0
    };

    let mut segments = Vec::with_capacity(STEPS);
    for i in 0..STEPS {
        let frac_step = i as f32 / (STEPS - 1) as f32;
        let color = if i < filled { fill } else { track };
        segments.push(
            div()
                .id(("seek", i))
                .flex_1()
                .h_full()
                .bg(color)
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

    div()
        .flex_1()
        .h(px(thickness))
        .rounded_full()
        .overflow_hidden()
        .flex()
        .children(segments)
}

fn np_seek_row(
    position: i64,
    duration: i64,
    thickness: f32,
    cx: &mut Context<FlacBox>,
) -> impl IntoElement {
    let time_el = |ms: i64, right: bool| {
        div()
            .w(px(46.))
            .text_size(px(10.5))
            .text_color(rgb(MUTED))
            .when(right, |el| el.flex().justify_end())
            .child(fmt_time(ms))
    };
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .child(time_el(position, false))
        .child(seek_track(
            position,
            duration,
            rgb(ACCENT),
            rgb(SURFACE_HI),
            thickness,
            cx,
        ))
        .child(time_el(duration, true))
}

fn circle_transport(
    id: &'static str,
    glyph: &'static str,
    size: f32,
    primary: bool,
    cx: &mut Context<FlacBox>,
    f: impl Fn(&mut FlacBox) + 'static + Clone,
) -> impl IntoElement {
    let (bg_color, glyph_color) = if primary {
        (ACCENT, BG)
    } else {
        (SURFACE_HI, TEXT)
    };
    div()
        .id(id)
        .size(px(size))
        .rounded_full()
        .bg(rgb(bg_color))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.opacity(0.9))
        .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
            f(this);
            cx.notify();
        }))
        .child(icon(glyph, size * 0.36, glyph_color))
}

fn transport_row(playing: bool, size: f32, cx: &mut Context<FlacBox>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_5()
        .child(circle_transport(
            "np-prev",
            "⏮",
            size * 0.72,
            false,
            cx,
            |this| {
                this.player.send(Cmd::Prev);
            },
        ))
        .child(circle_transport(
            "np-play",
            if playing { "▮▮" } else { "▶" },
            size,
            true,
            cx,
            |this| {
                this.toggle_play();
            },
        ))
        .child(circle_transport(
            "np-next",
            "⏭",
            size * 0.72,
            false,
            cx,
            |this| {
                this.player.send(Cmd::Next);
            },
        ))
}

fn secondary_row(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_4()
        .child(round_btn("np-shuffle", "⇄", this.shuffle, cx, |this| {
            this.shuffle = !this.shuffle;
            this.player.send(Cmd::SetShuffle(this.shuffle));
        }))
        .child(repeat_button("np-repeat", this, cx))
}

fn chip_btn(
    id: &'static str,
    label: &'static str,
    glyph: &'static str,
    active: bool,
    cx: &mut Context<FlacBox>,
    f: impl Fn(&mut FlacBox) + 'static + Clone,
) -> impl IntoElement {
    let color = if active { ACCENT } else { MUTED };
    div()
        .id(id)
        .h(px(28.))
        .px_2()
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap_1()
        .cursor_pointer()
        .when(active, |el| el.bg(rgb(SURFACE_HI)))
        .hover(|s| s.bg(rgb(SURFACE_HOVER)))
        .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
            f(this);
            cx.notify();
        }))
        .child(icon(glyph, 12., color))
        .child(
            div()
                .text_size(px(10.5))
                .text_color(rgb(color))
                .child(label),
        )
}

fn quality_pill(quality: &str) -> impl IntoElement {
    div()
        .h(px(22.))
        .px_3()
        .rounded_full()
        .bg(rgba(0xffffff10))
        .flex()
        .items_center()
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(ACCENT))
                .child(quality.to_string()),
        )
}

fn np_top_bar(this: &mut FlacBox, wide: bool, cx: &mut Context<FlacBox>) -> impl IntoElement {
    div()
        .h(px(52.))
        .px_4()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .id("back-btn")
                .h(px(30.))
                .px_3()
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap_1()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
                    this.show_player = false;
                    cx.notify();
                }))
                .child(icon("⌄", 16., MUTED))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child("Biblioteca"),
                ),
        )
        .when(wide, |el| {
            el.child(if this.is_internet_current() {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(chip_btn(
                        "np-audio-btn",
                        "AUDIO",
                        "▷",
                        !this.video.active,
                        cx,
                        |this| {
                            this.start_audio_internet();
                        },
                    ))
                    .child(chip_btn(
                        "np-video-btn",
                        "VIDEO",
                        "▣",
                        this.video.active,
                        cx,
                        |this| {
                            this.start_video_internet();
                        },
                    ))
                    .into_any_element()
            } else {
                div()
                    .text_size(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("R E P R O D U C I E N D O")
                    .into_any_element()
            })
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .when(this.lyrics_available() && !this.video.active, |el| {
                    el.child(chip_btn(
                        "np-lyrics-btn",
                        "LETRA",
                        "♫",
                        false,
                        cx,
                        |this| {
                            this.toggle_np_mode();
                        },
                    ))
                })
                .when(!this.video.active, |el| {
                    el.child(chip_btn(
                        "np-queue-btn",
                        "COLA",
                        "☰",
                        this.show_queue,
                        cx,
                        |this| {
                            this.show_queue = !this.show_queue;
                        },
                    ))
                }),
        )
}

fn queue_panel(this: &FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let rows = this
        .queue
        .iter()
        .enumerate()
        .map(|(i, _path)| {
            let is_current = i as i32 == this.snap.index;
            let name = this.queue_label(i);
            div()
                .id(("queue", i))
                .h(px(32.))
                .px_2()
                .rounded(px(5.))
                .flex()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .when(is_current, |el| el.bg(rgb(SURFACE_HI)))
                .hover(|s| s.bg(rgb(SURFACE_HOVER)))
                .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                    this.player.send(Cmd::PlayIndex(i));
                    cx.notify();
                }))
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
                .children(rows),
        )
}

fn lyrics_lines(this: &FlacBox) -> Vec<AnyElement> {
    let Some(lyrics) = &this.lyrics else {
        let message = match this.lyrics_status {
            LyricsStatus::Loading => "Buscando letra…",
            LyricsStatus::Empty => "Sin letra sincronizada disponible",
            _ => "Nada sonando",
        };
        return vec![div()
            .text_size(px(13.))
            .text_color(rgb(MUTED))
            .child(message)
            .into_any_element()];
    };
    if lyrics.lines.is_empty() {
        return vec![div()
            .text_size(px(13.))
            .text_color(rgb(MUTED))
            .child("Sin letra disponible")
            .into_any_element()];
    }

    let active = this.lyrics_active.unwrap_or(0).min(lyrics.lines.len() - 1);
    let window = 4usize;
    let start = active.saturating_sub(window);
    let end = (active + window + 1).min(lyrics.lines.len());

    lyrics
        .lines
        .iter()
        .enumerate()
        .skip(start)
        .take(end - start)
        .map(|(i, line)| {
            let is_active = this.lyrics_active == Some(i);
            let distance = this
                .lyrics_active
                .map(|active| active.abs_diff(i))
                .unwrap_or(99);
            let (size, color, opacity, weight) = if is_active {
                (27., TEXT, 1.0, FontWeight::SEMIBOLD)
            } else if distance == 1 {
                (16., TEXT, 0.8, FontWeight::NORMAL)
            } else if distance == 2 {
                (13.5, TEXT, 0.55, FontWeight::NORMAL)
            } else {
                (12.5, MUTED, 0.5, FontWeight::NORMAL)
            };
            div()
                .id(("lyric", i))
                .w_full()
                .px_7()
                .py_2()
                .text_size(px(size))
                .text_color(rgb(color))
                .opacity(opacity)
                .font_weight(weight)
                .child(if line.text.trim().is_empty() {
                    "·".to_string()
                } else {
                    line.text.clone()
                })
                .into_any_element()
        })
        .collect()
}

fn lyrics_source_row(this: &FlacBox, cx: &mut Context<FlacBox>) -> impl IntoElement {
    let chip =
        |mode: LyricsMode, label: &'static str, this: &FlacBox, cx: &mut Context<FlacBox>| {
            let active = this.lyrics_mode == mode;
            div()
                .id(label)
                .h(px(20.))
                .px_2()
                .rounded(px(4.))
                .flex()
                .items_center()
                .cursor_pointer()
                .when(active, |el| el.bg(rgba(0xffffff14)))
                .on_click(cx.listener(move |this, _ev: &ClickEvent, _w, cx| {
                    this.lyrics_mode = mode;
                    this.lyrics_track = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .text_size(px(9.5))
                        .text_color(rgb(if active { ACCENT } else { MUTED }))
                        .child(label),
                )
        };
    div()
        .flex()
        .items_center()
        .justify_center()
        .gap_1()
        .child(chip(LyricsMode::Auto, "AUTO", this, cx))
        .child(chip(LyricsMode::Embedded, "EMBEBIDA", this, cx))
        .child(chip(LyricsMode::Sidecar, ".LRC", this, cx))
        .child(chip(LyricsMode::Cache, "CACHE", this, cx))
        .child(chip(LyricsMode::Online, "ONLINE", this, cx))
}

fn lyrics_view(this: &mut FlacBox, cx: &mut Context<FlacBox>) -> AnyElement {
    let current = this.np_track().cloned();
    let color = this.current_art_color();
    let position = this.snap.position_ms;
    let duration = this.snap.duration_ms;
    let playing = this.snap.status == STATUS_PLAYING;

    let (title, artist) = match &current {
        Some(t) => (t.title.clone(), t.artist.clone()),
        None => ("Nada sonando".to_string(), String::new()),
    };

    let pill = div()
        .id("np-lyrics-pill")
        .h(px(60.))
        .pl_2()
        .pr_4()
        .rounded(px(30.))
        .bg(rgba(0x00000073))
        .flex()
        .items_center()
        .gap_3()
        .cursor_pointer()
        .on_click(cx.listener(|this, _ev: &ClickEvent, _w, cx| {
            this.np_mode = NpMode::Cover;
            cx.notify();
        }))
        .child(cover_el(this, current.as_ref().map(|t| &t.path), 44., 22.))
        .child(
            div()
                .flex()
                .flex_col()
                .max_w(px(320.))
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT))
                        .child(truncate(&title, 32)),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(truncate(&artist, 32)),
                ),
        )
        .child(icon("⌄", 14., MUTED));

    let badge = match this.lyrics.as_ref().map(|l| l.source) {
        Some(LyricSource::Embedded) => "embebida",
        Some(LyricSource::Sidecar) => ".lrc",
        Some(LyricSource::Lrclib) => "lrclib.net",
        Some(LyricSource::Plain) => "sin sincronizar",
        Some(LyricSource::Unknown) | None => "",
    };

    let lyric_children = if this.lyrics_available() {
        lyrics_lines(this)
    } else {
        vec![div()
            .text_size(px(13.))
            .text_color(rgb(MUTED))
            .child(match this.lyrics_status {
                LyricsStatus::Loading => "Buscando letra…",
                _ => "Sin letra sincronizada disponible",
            })
            .into_any_element()]
    };

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .bg(np_gradient(color, 0.38, 0.8))
        .child(div().h(px(18.)))
        .child(
            div()
                .px_5()
                .flex()
                .items_center()
                .justify_between()
                .child(pill)
                .child(
                    div()
                        .px_3()
                        .text_size(px(9.5))
                        .text_color(rgb(MUTED))
                        .child(badge),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .justify_center()
                .gap_1()
                .overflow_hidden()
                .children(lyric_children),
        )
        .child(
            div()
                .px_5()
                .pb_2()
                .flex()
                .justify_center()
                .child(lyrics_source_row(this, cx)),
        )
        .child(
            div()
                .px_5()
                .pb_5()
                .flex()
                .items_center()
                .gap_4()
                .child(circle_transport(
                    "np-lyr-play",
                    if playing { "▮▮" } else { "▶" },
                    56.,
                    true,
                    cx,
                    |this| {
                        this.toggle_play();
                    },
                ))
                .child(
                    div()
                        .flex_1()
                        .h(px(46.))
                        .rounded_full()
                        .bg(rgba(0x00000073))
                        .px_4()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(0xD8EDE1))
                                .child(fmt_time(position)),
                        )
                        .child(seek_track(
                            position,
                            duration,
                            rgb(0xE4FFF0),
                            rgba(0xffffff2a),
                            5.,
                            cx,
                        ))
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(0xD8EDE1))
                                .child(fmt_time(duration)),
                        ),
                ),
        )
        .into_any_element()
}

fn portada_view(this: &mut FlacBox, window: &Window, cx: &mut Context<FlacBox>) -> AnyElement {
    let current = this.np_track().cloned();
    let color = this.current_art_color();
    let position = this.snap.position_ms;
    let duration = this.snap.duration_ms;
    let playing = this.snap.status == STATUS_PLAYING;

    let w = window.viewport_size().width;
    let wide = w >= px(1060.);
    let mid = !wide && w >= px(720.);
    let show_queue = this.show_queue && wide && !this.video.active;

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

    let art_path = current.as_ref().map(|t| &t.path);
    let bg = np_gradient(color, 0.74, 0.92);

    let lyrics_panel_el = |this: &mut FlacBox| {
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .justify_center()
            .gap_1()
            .border_l_1()
            .border_color(rgb(BORDER))
            .when(!this.lyrics_available(), |el| {
                el.items_center()
                    .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(
                        match this.lyrics_status {
                            LyricsStatus::Loading => "Buscando letra…",
                            LyricsStatus::Empty => "Sin letra sincronizada disponible",
                            _ => "Sin letra sincronizada",
                        },
                    ))
            })
            .children(if this.lyrics_available() {
                lyrics_lines(this)
            } else {
                Vec::new()
            })
    };

    if wide {
        let main = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(bg)
            .child(np_top_bar(this, true, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .child(div().flex_1().flex().items_center().justify_center().child(
                        if this.video.active {
                            video_slot_el(this, 640.)
                        } else {
                            cover_el(this, art_path, 380., 14.)
                        },
                    ))
                    .when(this.lyrics_available(), |el| {
                        el.child(lyrics_panel_el(this))
                    }),
            )
            .child(
                div()
                    .px_6()
                    .pb_5()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_8()
                    .child(
                        div()
                            .w(px(440.))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .items_start()
                            .child(
                                div()
                                    .text_size(px(24.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(truncate(&title, 40)),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(MUTED))
                                    .child(truncate(&format!("{artist} · {album}"), 58)),
                            )
                            .child(quality_pill(&quality)),
                    )
                    .child(
                        div()
                            .w(px(440.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_3()
                            .child(seek_area(this, position, duration, 6., cx))
                            .child(transport_area(this, playing, 68., cx))
                            .when(!this.video.active, |el| el.child(secondary_row(this, cx))),
                    ),
            );
        return div()
            .flex_1()
            .min_h_0()
            .flex()
            .child(main)
            .when(show_queue, |el| el.child(queue_panel(this, cx)))
            .into_any_element();
    }

    if mid {
        return div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(bg)
            .child(np_top_bar(this, false, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .px_6()
                    .child(if this.video.active {
                        video_slot_el(this, 520.)
                    } else {
                        cover_el(this, art_path, 240., 12.)
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(truncate(&title, 36)),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(MUTED))
                                    .child(truncate(&format!("{artist} · {album}"), 50)),
                            )
                            .child(quality_pill(&quality)),
                    )
                    .child(
                        div()
                            .w(px(480.))
                            .child(seek_area(this, position, duration, 6., cx)),
                    )
                    .child(transport_area(this, playing, 64., cx))
                    .when(!this.video.active, |el| el.child(secondary_row(this, cx))),
            )
            .into_any_element();
    }

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .bg(bg)
        .child(np_top_bar(this, false, cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .px_4()
                .child(if this.video.active {
                    video_slot_el(this, 320.)
                } else {
                    cover_el(this, art_path, 185., 14.)
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(19.))
                                .font_weight(FontWeight::BOLD)
                                .child(truncate(&title, 28)),
                        )
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(MUTED))
                                .child(truncate(&artist, 36)),
                        ),
                )
                .child(seek_area(this, position, duration, 5., cx))
                .child(transport_area(this, playing, 60., cx))
                .when(!this.video.active, |el| el.child(secondary_row(this, cx))),
        )
        .into_any_element()
}

pub fn now_playing(
    this: &mut FlacBox,
    window: &mut Window,
    cx: &mut Context<FlacBox>,
) -> AnyElement {
    match this.np_mode {
        NpMode::Lyrics => lyrics_view(this, cx),
        NpMode::Cover => portada_view(this, window, cx),
    }
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
                                "enter" => this.create_playlist(),
                                "escape" => {
                                    this.playlist_modal_open = false;
                                    this.playlist_input.clear();
                                }
                                _ => {
                                    edit_text_field(&mut this.playlist_input, ev, cx);
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
                                "enter" => {
                                    let raw = expand_home(&this.folder_input);
                                    if !raw.is_empty() {
                                        this.add_folder(std::path::PathBuf::from(raw), cx);
                                    }
                                }
                                "escape" => {
                                    this.add_folder_open = false;
                                    this.folder_input.clear();
                                }
                                _ => {
                                    edit_text_field(&mut this.folder_input, ev, cx);
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
