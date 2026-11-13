pub const BG: u32 = 0x0E1512;
pub const SURFACE: u32 = 0x16211C;
pub const SURFACE_HI: u32 = 0x1F2E28;
pub const SURFACE_HOVER: u32 = 0x24352E;
pub const BORDER: u32 = 0x243A32;
pub const ACCENT: u32 = 0x8EE6B8;
pub const ACCENT_DIM: u32 = 0x33574A;
pub const TEXT: u32 = 0xE8F1EC;
pub const MUTED: u32 = 0x8FA39B;

pub fn mix_rgb(a: u32, b: u32, t: f32) -> u32 {
    let ar = (a >> 16) & 0xff;
    let ag = (a >> 8) & 0xff;
    let ab = a & 0xff;
    let br = (b >> 16) & 0xff;
    let bg = (b >> 8) & 0xff;
    let bb = b & 0xff;
    let r = (ar as f32 + (br as f32 - ar as f32) * t).round() as u32;
    let g = (ag as f32 + (bg as f32 - ag as f32) * t).round() as u32;
    let bl = (ab as f32 + (bb as f32 - ab as f32) * t).round() as u32;
    (r << 16) | (g << 8) | bl
}

pub fn fmt_time(ms: i64) -> String {
    let total = (ms.max(0) / 1000) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}
