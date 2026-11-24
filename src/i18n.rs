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
    Arabic,
    Portuguese,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::English, Self::Portuguese, Self::Arabic];
    pub fn key(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Arabic => "ar",
            // YouTube's language code for Brazilian Portuguese.
            Self::Portuguese => "pt",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Arabic => "العربية",
            Self::Portuguese => "Português (Brasil)",
        }
    }
    pub fn from_key(key: &str) -> Self {
        match key {
            "ar" => Self::Arabic,
            "pt" => Self::Portuguese,
            _ => Self::English,
        }
    }
}

pub fn set_language(language: Language) {
    LANGUAGE.store(language as u8, Ordering::Relaxed);
}

pub fn tr(text: &str) -> &str {
    const ARABIC: u8 = Language::Arabic as u8;
    const PORTUGUESE: u8 = Language::Portuguese as u8;
    match LANGUAGE.load(Ordering::Relaxed) {
        ARABIC => arabic(text),
        PORTUGUESE => crate::i18n_pt::translate(text),
        _ => text,
    }
}

fn arabic(text: &str) -> &str {
    match text {
        "Home" => "الرئيسية",
        "History" => "السجل",
        "Channels" => "القنوات",
        "Search" => "بحث",
        "Settings" => "الإعدادات",
        "Video" => "الفيديو",
        "Videos" => "الفيديوهات",
        "Playback" => "التشغيل",
        "About" => "حول التطبيق",
        "Highest quality" => "أعلى جودة",
        "Lower plays smoother on slow connections" => "الجودة الأقل تناسب الاتصال البطيء",
        "Preferred codec" => "ترميز الفيديو المفضل",
        "VP9 suits most TVs; AV1 needs newer graphics" => {
            "يناسب VP9 معظم الأجهزة، ويحتاج AV1 إلى بطاقة أحدث"
        }
        "When the video and the TV have it" => "عندما يدعمه الفيديو والتلفزيون",
        "Subtitles" => "الترجمة",
        "Subtitle language" => "لغة الترجمة",
        "Interface language" => "لغة الواجهة",
        "When a video ends" => "عند انتهاء الفيديو",
        "Play the next video" => "تشغيل الفيديو التالي",
        "Play it again" => "إعادة التشغيل",
        "Stop" => "إيقاف",
        "Buffer" => "التخزين المؤقت",
        "Bigger rides out a shaky connection, smaller uses less memory" => {
            "الحجم الأكبر يناسب الاتصال غير المستقر"
        }
        "Small" => "صغير",
        "Medium" => "متوسط",
        "Large" => "كبير",
        "Country" => "البلد",
        "For recommendations, trending and news" => "للاقتراحات والمحتوى الرائج والأخبار",
        "Skip sponsors and more" => "تخطي الرعاية والمقاطع الأخرى",
        "Segments marked by SponsorBlock viewers" => "مقاطع حددها مستخدمو SponsorBlock",
        "Player statistics" => "إحصاءات المشغل",
        "Decoder, dropped frames and memory over the video" => {
            "عرض وحدة فك الترميز والإطارات المفقودة والذاكرة"
        }
        "Update the player" => "تحديث المشغل",
        "Updating…" => "جارٍ التحديث…",
        "Now playing" => "قيد التشغيل الآن",
        "Return to video" => "العودة إلى الفيديو",
        "Playing" => "قيد التشغيل",
        "Play" => "تشغيل",
        "Start over" => "التشغيل من البداية",
        "Go to channel" => "الانتقال إلى القناة",
        "No channel for this one" => "لا توجد قناة لهذا الفيديو",
        "For you" => "مقترح لك",
        "People also watched" => "شاهد الآخرون أيضاً",
        "Shorts" => "فيديوهات قصيرة",
        "Recent searches" => "عمليات البحث الأخيرة",
        "Search YouTube" => "البحث في YouTube",
        "Search to get started" => "ابحث للبدء",
        "Nothing found" => "لا توجد نتائج",
        "Nothing here yet" => "لا يوجد محتوى بعد",
        "Down for results" => "اضغط لأسفل لعرض النتائج",
        "Press OK to try again" => "اضغط موافق للمحاولة مجدداً",
        "Getting the player ready (first start)…" => "جارٍ إعداد المشغل للتشغيل الأول…",
        "space" => "مسافة",
        "delete" => "حذف",
        "LIVE" => "مباشر",
        "Short" => "قصير",
        "Sponsor" => "رعاية",
        "Unpaid or self promotion" => "ترويج شخصي أو غير مدفوع",
        "Reminder to subscribe" => "تذكير بالاشتراك",
        "Intro" => "مقدمة",
        "Credits and end cards" => "تترات وشاشات النهاية",
        "Preview or recap" => "معاينة أو ملخص",
        "Non-music part in a music video" => "جزء غير موسيقي في فيديو موسيقي",
        "Off-topic filler" => "محتوى خارج الموضوع",
        "Hook or greeting" => "افتتاحية أو تحية",
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

    #[test]
    fn portuguese_covers_every_interface_text() {
        let source = include_str!("i18n.rs");
        let start = source.find("fn arabic(text").unwrap();
        let end = start + source[start..].find("_ => text,").unwrap();
        let keys: Vec<&str> = source[start..end]
            .lines()
            .filter_map(|l| l.trim().strip_prefix('"')?.split_once("\" =>"))
            .map(|(key, _)| key)
            .collect();
        assert!(keys.len() > 40);
        let same_in_both = ["Buffer", "Shorts", "Short"];
        for key in keys {
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
