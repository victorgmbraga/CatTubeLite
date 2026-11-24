//! Interface language and bidirectional text layout.

use eframe::egui;
use std::borrow::Cow;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, LazyLock};
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

static LANGUAGE: AtomicU8 = AtomicU8::new(Language::English as u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    Portuguese,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::Portuguese];
    pub fn key(self) -> &'static str {
        match self {
            Self::English => "en",
            // YouTube's language code for Brazilian Portuguese.
            Self::Portuguese => "pt",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Portuguese => "Português (Brasil)",
        }
    }
    pub fn from_key(key: &str) -> Self {
        match key {
            "pt" => Self::Portuguese,
            _ => Self::English,
        }
    }
}

pub fn set_language(language: Language) {
    LANGUAGE.store(language as u8, Ordering::Relaxed);
}

pub fn tr(text: &str) -> &str {
    const PORTUGUESE: u8 = Language::Portuguese as u8;
    match LANGUAGE.load(Ordering::Relaxed) {
        PORTUGUESE => crate::i18n_pt::translate(text),
        _ => text,
    }
}

pub fn is_rtl(text: &str) -> bool {
    if text.is_ascii() {
        return false;
    }
    BidiInfo::new(text, None)
        .paragraphs
        .first()
        .is_some_and(|p| p.level.is_rtl())
}

static SHAPER: LazyLock<arabic_reshaper::ArabicReshaper<'static>> = LazyLock::new(|| {
    let mut shaper = arabic_reshaper::ArabicReshaper::new();
    shaper.configuration.insert("delete_harakat", false);
    shaper.configuration.insert("shift_harakat_position", true);
    shaper
});

fn display(text: &str) -> Cow<'_, str> {
    if text.is_ascii() || !BidiInfo::new(text, None).has_rtl() {
        return Cow::Borrowed(text);
    }
    let mut visual = String::new();
    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let shaped = SHAPER.reshape(content);
        let bidi = BidiInfo::new(&shaped, None);
        for paragraph in &bidi.paragraphs {
            visual.push_str(&bidi.reorder_line(paragraph, paragraph.range.clone()));
        }
        if line.ends_with('\n') {
            visual.push('\n');
        }
    }
    Cow::Owned(visual)
}

/// Keep model text logical; shape and reorder only at the drawing boundary.
pub fn layout(
    painter: &egui::Painter,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
) -> Arc<egui::Galley> {
    let visual = display(text);
    if matches!(visual, Cow::Borrowed(_)) {
        return painter.layout_no_wrap(text.to_owned(), font, color);
    }
    let mut job = egui::text::LayoutJob::default();
    let format = egui::TextFormat::simple(font, color);
    // The renderer assumes ascending clusters. Isolate visual graphemes so
    // its shaper cannot reverse a run that has already been reordered.
    for grapheme in visual.graphemes(true) {
        let start = job.text.len();
        job.text.push_str(grapheme);
        job.sections.push(egui::text::LayoutSection {
            leading_space: 0.0,
            byte_range: start.into()..job.text.len().into(),
            format: format.clone(),
        });
    }
    painter.layout_job(job)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every text the interface passes through [`tr`].
    const INTERFACE_TEXTS: &[&str] = &[
        "Home",
        "History",
        "Channels",
        "Search",
        "Settings",
        "Video",
        "Videos",
        "Playback",
        "About",
        "Highest quality",
        "Lower plays smoother on slow connections",
        "Preferred codec",
        "VP9 suits most TVs; AV1 needs newer graphics",
        "When the video and the TV have it",
        "Subtitles",
        "Subtitle language",
        "Interface language",
        "When a video ends",
        "Play the next video",
        "Play it again",
        "Stop",
        "Buffer",
        "Bigger rides out a shaky connection, smaller uses less memory",
        "Small",
        "Medium",
        "Large",
        "Country",
        "For recommendations, trending and news",
        "Skip sponsors and more",
        "Segments marked by SponsorBlock viewers",
        "Player statistics",
        "Decoder, dropped frames and memory over the video",
        "Update the player",
        "Updating…",
        "Now playing",
        "Return to video",
        "Playing",
        "Play",
        "Start over",
        "Go to channel",
        "No channel for this one",
        "For you",
        "People also watched",
        "Shorts",
        "Recent searches",
        "Search YouTube",
        "Search to get started",
        "Nothing found",
        "Nothing here yet",
        "Down for results",
        "Press OK to try again",
        "Getting the player ready (first start)…",
        "space",
        "delete",
        "LIVE",
        "Short",
        "Sponsor",
        "Unpaid or self promotion",
        "Reminder to subscribe",
        "Intro",
        "Credits and end cards",
        "Preview or recap",
        "Non-music part in a music video",
        "Off-topic filler",
        "Hook or greeting",
    ];

    #[test]
    fn portuguese_covers_every_interface_text() {
        let keys = INTERFACE_TEXTS;
        let same_in_both = ["Buffer", "Shorts", "Short"];
        for &key in keys {
            assert!(
                crate::i18n_pt::translate(key) != key || same_in_both.contains(&key),
                "no Portuguese for {key:?}"
            );
        }
    }

    #[test]
    fn language_keys_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::from_key(language.key()), language);
        }
        assert_eq!(Language::from_key("xx"), Language::English);
    }

    #[test]
    fn rendered_arabic_and_mixed_text_follow_visual_order() {
        let ctx = egui::Context::default();
        crate::theme::setup(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let font = egui::FontId::proportional(24.0);
            let logical = "بحث YouTube 2026";
            let galley = layout(ui.painter(), logical, font.clone(), egui::Color32::WHITE);
            let visible: String = galley.rows[0]
                .glyphs
                .iter()
                .filter(|g| !g.chr.is_control() && g.chr != '\u{200f}')
                .map(|g| g.chr)
                .collect();
            assert!(visible.starts_with("YouTube 2026"), "{visible:?}");
            assert_eq!(visible, display(logical), "{visible:?}");
            assert!(
                visible
                    .chars()
                    .any(|c| ('\u{fe70}'..='\u{feff}').contains(&c))
            );
            assert_eq!(logical, "بحث YouTube 2026");
            let marked = layout(ui.painter(), "مَرْحَبًا\nالعربية", font, egui::Color32::WHITE);
            assert_eq!(marked.rows.len(), 2);
            assert!(marked.rows.iter().all(|r| !r.glyphs.is_empty()));
            assert!(!is_rtl("YouTube 2026"));
            assert!(is_rtl("بحث YouTube 2026"));
            assert!(matches!(
                display("YouTube 2026"),
                Cow::Borrowed("YouTube 2026")
            ));
        })
        .drop_without_applying_deltas();
    }
}
