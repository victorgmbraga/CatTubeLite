//! Shared visual tokens and global egui setup.
//!
//! Design direction: **dark glass**. A deep
//! obsidian backdrop lit by soft colour glows; translucent "glass"
//! surfaces with hairline borders; pill-shaped controls; focus = a lift,
//! brighter glass, a bright specular ring and a coloured bloom underneath;
//! one accent colour (`accent` in settings.conf) for selection,
//! progress and blooms. Calm, label-first copy, no marketing sentences.
//!
//! Colours are built with [`rgba`] from `(r, g, b)` tuples. Never derive a
//! new colour from `Color32::r()/g()/b()` of a translucent colour: egui
//! stores colours premultiplied, so those channels are already darkened.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub type Rgb = (u8, u8, u8);

/// BTV9: no translucent surfaces, shadows or sheens (the Mali-450 pays for
/// every blended layer). See [`rgba`].
pub const FLAT: bool = true;

/// Colour from an RGB tuple and alpha. With [`FLAT`] the alpha is resolved
/// against black, so the result is solid; 0 stays invisible.
pub fn rgba(c: Rgb, a: u8) -> Color32 {
    if FLAT {
        if a == 0 {
            return Color32::TRANSPARENT;
        }
        let m = |x: u8, y: u8| ((x as u32 * a as u32 + y as u32 * (255 - a as u32)) / 255) as u8;
        return Color32::from_rgb(m(c.0, BLACK.0), m(c.1, BLACK.1), m(c.2, BLACK.2));
    }
    Color32::from_rgba_unmultiplied(c.0, c.1, c.2, a)
}

pub fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

// ---------------------------------------------------------------- base

/// Backdrop, top and bottom of the vertical gradient.
pub const BG_TOP: Rgb = (15, 18, 28);
pub const BG_BOTTOM: Rgb = (6, 8, 13);
/// Primary text.
pub const INK: Rgb = (245, 246, 250);
pub const WHITE: Rgb = (255, 255, 255);
pub const BLACK: Rgb = (0, 0, 0);

pub fn text() -> Color32 {
    rgb(INK)
}
/// Secondary text (labels, values).
pub fn text2() -> Color32 {
    rgba(INK, 165)
}
/// Tertiary text (hints, disabled).
pub fn text3() -> Color32 {
    rgba(INK, 105)
}
/// Glass surface fill (panels).
pub fn glass() -> Color32 {
    rgba(WHITE, 15)
}
/// Opaque panel colour for sheets and side panels.
pub const PANEL: Rgb = (20, 24, 36);
pub fn border() -> Color32 {
    rgba(WHITE, 22)
}
pub fn border_hi() -> Color32 {
    rgba(WHITE, 48)
}
/// Modal scrim.
pub fn scrim() -> Color32 {
    rgba(BLACK, 165)
}

// -------------------------------------------------------------- accent

/// Accent colours, by the name used in settings.conf.
pub const ACCENTS: &[(&str, Rgb)] = &[
    ("violet", (139, 124, 255)),
    ("periwinkle", (139, 164, 254)),
    ("blue", (77, 142, 255)),
    ("teal", (45, 212, 191)),
    ("green", (74, 222, 128)),
    ("amber", (251, 191, 36)),
    ("rose", (251, 113, 133)),
    // The red of YouTube's own logo (#FF0000).
    ("red", (255, 0, 0)),
    ("silver", (214, 220, 232)),
];

static ACCENT: AtomicU32 = AtomicU32::new(0x8B7CFF);

pub fn accent_rgb_of(name: &str) -> Rgb {
    ACCENTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
        .unwrap_or(ACCENTS[0].1)
}

/// Switch the interface accent (takes effect on the next frame).
pub fn set_accent(name: &str) {
    let (r, g, b) = accent_rgb_of(name);
    ACCENT.store(u32::from_be_bytes([0, r, g, b]), Ordering::Relaxed);
}

pub fn accent_rgb() -> Rgb {
    let [_, r, g, b] = ACCENT.load(Ordering::Relaxed).to_be_bytes();
    (r, g, b)
}

pub fn accent() -> Color32 {
    rgb(accent_rgb())
}

pub fn accent_a(a: u8) -> Color32 {
    rgba(accent_rgb(), a)
}

/// Text colour that reads well on a solid accent fill.
pub fn on_accent() -> Color32 {
    let (r, g, b) = accent_rgb();
    let luma = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
    if luma > 150.0 {
        rgb((12, 13, 20))
    } else {
        rgb(WHITE)
    }
}

// --------------------------------------------------------------- fonts

/// Font family names. Always defined by [`setup`] (falling back to the
/// regular face), so using them can never panic.
pub const MEDIUM: &str = "cattube-medium";
pub const SEMIBOLD: &str = "cattube-semibold";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    Semibold,
}

/// Inter (SIL Open Font License, `assets/fonts/LICENSE-Inter.txt`), built in.
fn load_face(w: Weight) -> Option<FontData> {
    let bytes: &'static [u8] = match w {
        Weight::Regular => include_bytes!("../assets/fonts/Inter-Regular.ttf"),
        Weight::Medium => include_bytes!("../assets/fonts/Inter-Medium.ttf"),
        Weight::Semibold => include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    };
    Some(FontData::from_static(bytes))
}

/// Install fonts and base visuals. Call once from the app constructor.
pub fn setup(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let base = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut family = |w: Weight, key: &str| -> Vec<String> {
        let mut list = base.clone();
        // Regular is the fallback for the heavier weights.
        for (k, weight) in [(key, w), ("cattube-regular", Weight::Regular)] {
            if fonts.font_data.contains_key(k) {
                list.insert(0, k.to_string());
                break;
            }
            if let Some(fd) = load_face(weight) {
                fonts.font_data.insert(k.to_string(), Arc::new(fd));
                list.insert(0, k.to_string());
                break;
            }
        }
        list
    };
    let regular = family(Weight::Regular, "cattube-regular");
    let medium = family(Weight::Medium, "cattube-medium-face");
    let semibold = family(Weight::Semibold, "cattube-semibold-face");
    fonts.families.insert(FontFamily::Proportional, regular);
    fonts
        .families
        .insert(FontFamily::Name(MEDIUM.into()), medium);
    fonts
        .families
        .insert(FontFamily::Name(SEMIBOLD.into()), semibold);
    ctx.set_fonts(fonts);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = rgb(BG_BOTTOM);
    visuals.window_fill = rgb(BG_TOP);
    visuals.override_text_color = Some(text());
    ctx.set_visuals(visuals);
}
