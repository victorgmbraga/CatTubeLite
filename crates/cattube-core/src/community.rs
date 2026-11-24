//! Community data next to YouTube's own: SponsorBlock
//! (segments to skip: sponsors, intros, …).
//!
//! | What | Request |
//! |---|---|
//! | segments | `GET https://sponsor.ajay.app/api/skipSegments?videoID=…&categories=[…]` (404 = none) |

use crate::mpv::{Json, parse_json};

/// SponsorBlock's categories, in the order Settings lists them, with
/// whether CatTube skips each by default.
pub const CATEGORIES: [(&str, &str, bool); 9] = [
    ("sponsor", "Sponsor", true),
    ("selfpromo", "Unpaid or self promotion", true),
    ("interaction", "Reminder to subscribe", true),
    ("intro", "Intro", true),
    ("outro", "Credits and end cards", true),
    ("preview", "Preview or recap", false),
    ("music_offtopic", "Non-music part in a music video", true),
    ("filler", "Off-topic filler", false),
    ("hook", "Hook or greeting", false),
];

pub fn category_label(key: &str) -> &'static str {
    CATEGORIES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map_or("Segment", |(_, l, _)| l)
}

pub fn default_categories() -> Vec<String> {
    CATEGORIES
        .iter()
        .filter(|(_, _, on)| *on)
        .map(|(k, _, _)| k.to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub category: String,
}

pub fn segments_url(video_id: &str, categories: &[String]) -> Option<String> {
    let ok_id = video_id.len() == 11
        && video_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !ok_id || categories.is_empty() {
        return None;
    }
    let cats: Vec<String> = categories
        .iter()
        .filter(|c| CATEGORIES.iter().any(|(k, _, _)| k == c))
        .map(|c| format!("%22{c}%22"))
        .collect();
    Some(format!(
        // Brackets encoded: curl reads raw [] as a range of addresses.
        "https://sponsor.ajay.app/api/skipSegments?videoID={video_id}&categories=%5B{}%5D",
        cats.join(",")
    ))
}

/// Segments to skip (actionType `skip`), sorted; anything unreadable is
/// none.
pub fn parse_segments(body: &str) -> Vec<Segment> {
    let Some(Json::Arr(list)) = parse_json(body.trim()) else {
        return Vec::new();
    };
    let mut v: Vec<Segment> = list
        .iter()
        .filter(|s| {
            s.get("actionType")
                .and_then(Json::as_str)
                .is_none_or(|a| a == "skip")
        })
        .filter_map(|s| {
            let seg = s.get("segment")?.as_arr()?;
            let start = seg.first()?.as_f64()?;
            let end = seg.get(1)?.as_f64()?;
            (end > start + 0.5).then(|| Segment {
                start,
                end,
                category: s
                    .get("category")
                    .and_then(Json::as_str)
                    .unwrap_or("sponsor")
                    .to_string(),
            })
        })
        .collect();
    v.sort_by(|a, b| a.start.total_cmp(&b.start));
    v
}

/// Where to jump to at `pos`: the end of a segment that `pos` is in and
/// that was not skipped before (`done` holds their starts). Segments running
/// to the end of the video are returned too so the player can loop or finish
/// cleanly instead of seeking past EOF.
pub fn skip_at(
    segs: &[Segment],
    pos: f64,
    duration: Option<f64>,
    done: &[f64],
) -> Option<(f64, String)> {
    segs.iter()
        .find(|s| {
            pos >= s.start
                && pos < s.end - 0.5
                && duration.is_none_or(|d| pos < d - 1.0)
                && !done.iter().any(|d| (d - s.start).abs() < 0.01)
        })
        .map(|s| (s.end, s.category.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_and_skipping() {
        assert_eq!(
            segments_url(
                "WhWc3b3KhnY",
                &["sponsor".into(), "bogus".into(), "intro".into()]
            )
            .unwrap(),
            "https://sponsor.ajay.app/api/skipSegments?videoID=WhWc3b3KhnY&categories=%5B%22sponsor%22,%22intro%22%5D"
        );
        assert_eq!(segments_url("bad", &default_categories()), None);
        let s = parse_segments(
            r#"[{"segment":[60.5,90],"category":"sponsor","actionType":"skip","UUID":"a"},
                {"segment":[5,20],"category":"intro","actionType":"skip"},
                {"segment":[100,100.2],"category":"sponsor"},
                {"segment":[30,40],"category":"poi_highlight","actionType":"poi"}]"#,
        );
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].category, "intro");
        assert_eq!(skip_at(&s, 3.0, Some(600.0), &[]), None);
        assert_eq!(
            skip_at(&s, 6.0, Some(600.0), &[]),
            Some((20.0, "intro".into()))
        );
        // Skipped once (the viewer may go back into it on purpose).
        assert_eq!(skip_at(&s, 6.0, Some(600.0), &[5.0]), None);
        // A segment running to the end is returned so the player can finish or loop cleanly,
        // unless playback is already within the final second.
        assert_eq!(
            skip_at(&s, 70.0, Some(90.5), &[]),
            Some((90.0, "sponsor".into()))
        );
        assert_eq!(skip_at(&s, 89.8, Some(90.5), &[]), None);
        assert!(parse_segments("Not Found").is_empty());
        assert_eq!(category_label("selfpromo"), "Unpaid or self promotion");
    }
}
