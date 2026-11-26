//! Interface language and text layout.

use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

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

/// Lay out one line of text for drawing.
pub fn layout(
    painter: &egui::Painter,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
) -> Arc<egui::Galley> {
    painter.layout_no_wrap(text.to_owned(), font, color)
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
        "Could not play this video",
        "The video took too long to start",
        "This video is not available",
        "YouTube blocked playback; try updating the player in Settings",
        "Could not reach YouTube",
        "This quality or codec is not available for this video",
        "Could not start video playback; check quality or codec in Settings",
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
}
