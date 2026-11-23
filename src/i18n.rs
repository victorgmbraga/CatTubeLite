//! Interface language and bidirectional text layout.

use eframe::egui;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use unicode_bidi::BidiInfo;
use unicode_segmentation::UnicodeSegmentation;

static ARABIC: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    English,
    Arabic,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::Arabic];
    pub fn key(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Arabic => "ar",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Arabic => "العربية",
        }
    }
    pub fn from_key(key: &str) -> Self {
        if key == "ar" {
            Self::Arabic
        } else {
            Self::English
        }
    }
}

pub fn set_language(language: Language) {
    ARABIC.store(language == Language::Arabic, Ordering::Relaxed);
}

pub fn tr(text: &str) -> &str {
    if !ARABIC.load(Ordering::Relaxed) {
        return text;
    }
    match text {
        "Home" => "الرئيسية",
        "Subscriptions" => "الاشتراكات",
        "History" => "السجل",
        "Playlists" => "قوائم التشغيل",
        "Channels" => "القنوات",
        "Search" => "بحث",
        "Settings" => "الإعدادات",
        "Accounts" => "الحسابات",
        "Account" => "الحساب",
        "Video" => "الفيديو",
        "Videos" => "الفيديوهات",
        "Playback" => "التشغيل",
        "About" => "حول التطبيق",
        "Phone remote" => "التحكم بالهاتف",
        "Add an account" => "إضافة حساب",
        "YouTube account" => "حساب YouTube",
        "Sign in with a code" => "تسجيل الدخول برمز",
        "Sign in to see" => "سجّل الدخول لعرض",
        "Sign in" => "تسجيل الدخول",
        "Sign in to YouTube" => "تسجيل الدخول إلى YouTube",
        "In use" => "قيد الاستخدام",
        "OK uses it · hold OK to sign out" => "اضغط موافق لاستخدامه، أو مطولاً لتسجيل الخروج",
        "Highest quality" => "أعلى جودة",
        "Lower plays smoother on slow connections" => "الجودة الأقل تناسب الاتصال البطيء",
        "Preferred codec" => "ترميز الفيديو المفضل",
        "VP9 suits most TVs; AV1 needs newer graphics" => {
            "يناسب VP9 معظم الأجهزة، ويحتاج AV1 إلى بطاقة أحدث"
        }
        "When the video and the TV have it" => "عندما يدعمه الفيديو والتلفزيون",
        "Speed" => "السرعة",
        "Also changed in the player, and kept" => "يمكن تغييرها في المشغل أيضاً",
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
        "Link a phone" => "ربط هاتف",
        "Pick and control videos from the YouTube app" => {
            "اختيار الفيديوهات والتحكم بها من تطبيق YouTube"
        }
        "Update the player" => "تحديث المشغل",
        "Updating…" => "جارٍ التحديث…",
        "Now playing" => "قيد التشغيل الآن",
        "Return to video" => "العودة إلى الفيديو",
        "Playing" => "قيد التشغيل",
        "Play" => "تشغيل",
        "Start over" => "التشغيل من البداية",
        "Like" => "إعجاب",
        "Dislike" => "عدم إعجاب",
        "Subscribe" => "اشتراك",
        "Unsubscribe" => "إلغاء الاشتراك",
        "Save to Watch later" => "الحفظ للمشاهدة لاحقاً",
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
        "Press OK for a new code" => "اضغط موافق لرمز جديد",
        "Getting a code…" => "جارٍ الحصول على رمز…",
        "Waiting for you to enter it…" => "في انتظار إدخال الرمز…",
        "Back to cancel" => "رجوع للإلغاء",
        "On your phone or computer, go to" => "على هاتفك أو حاسوبك، افتح",
        "and enter this code" => "وأدخل هذا الرمز",
        "Link your phone" => "ربط هاتفك",
        "Link with TV code" => "الربط برمز التلفزيون",
        "In the YouTube app on your phone, open Settings → Watch on TV" => {
            "في تطبيق YouTube على الهاتف، افتح الإعدادات ثم المشاهدة على التلفزيون"
        }
        "→ Link with TV code, and enter" => "ثم الربط برمز التلفزيون وأدخل",
        "The code changes every few minutes. A linked phone stays linked." => {
            "يتغير الرمز كل بضع دقائق، ويبقى الهاتف المرتبط متصلاً."
        }
        "Then pick a video on the phone: it plays here, and the phone stays in step." => {
            "اختر فيديو من الهاتف لتشغيله هنا والتحكم به."
        }
        "OK for a new code · Back to close" => "موافق لرمز جديد، رجوع للإغلاق",
        "Getting the player ready (first start)…" => "جارٍ إعداد المشغل للتشغيل الأول…",
        "space" => "مسافة",
        "delete" => "حذف",
        "LIVE" => "مباشر",
        "Short" => "قصير",
        "Choose the tab to sign in with a code" => "اختر تسجيل الدخول للحصول على رمز",
        "Sign in (the button at the top right) for your recommendations and subscriptions" => {
            "سجّل الدخول لعرض اقتراحاتك واشتراكاتك"
        }
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
