//! The last Home, kept on disk (`home.txt` in the cache folder) so the
//! app can show it at once on the next start while a fresh one loads.
//! Tab-separated lines; tied to the request context (language, country
//! and client), so a change there makes it unused.

use crate::youtube::{Kind, Page, Shelf, Tile};

const HEADER: &str = "cattube-home 1";

fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn kind_key(k: Kind) -> &'static str {
    match k {
        Kind::Video => "video",
        Kind::Short => "short",
        Kind::Live => "live",
        Kind::Channel => "channel",
        Kind::Playlist => "playlist",
    }
}

fn kind_from(s: &str) -> Option<Kind> {
    Some(match s {
        "video" => Kind::Video,
        "short" => Kind::Short,
        "live" => Kind::Live,
        "channel" => Kind::Channel,
        "playlist" => Kind::Playlist,
        _ => return None,
    })
}

fn opt(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// Home as text, for the request context `ctx`.
pub fn to_text(ctx: &str, page: &Page) -> String {
    let mut out = format!("{HEADER}\nctx\t{}\n", clean(ctx));
    for shelf in &page.shelves {
        out.push_str(&format!(
            "shelf\t{}\t{}\t{}\n",
            clean(&shelf.title),
            clean(shelf.continuation.as_deref().unwrap_or("")),
            clean(shelf.more.as_deref().unwrap_or(""))
        ));
        for t in &shelf.tiles {
            out.push_str(&format!(
                "tile\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                kind_key(t.kind),
                clean(&t.id),
                clean(&t.title),
                clean(&t.byline),
                clean(&t.channel_id),
                clean(&t.meta),
                clean(&t.badge),
                clean(&t.thumb)
            ));
        }
    }
    out
}

/// The Home kept in `text`, when it was saved for the same `ctx` and has
/// videos to show.
pub fn parse(ctx: &str, text: &str) -> Option<Page> {
    let mut lines = text.lines();
    if lines.next()? != HEADER || lines.next()? != format!("ctx\t{}", clean(ctx)) {
        return None;
    }
    let mut page = Page::default();
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let get = |i: usize| f.get(i).copied().unwrap_or("");
        match get(0) {
            "shelf" => page.shelves.push(Shelf {
                title: get(1).to_string(),
                tiles: Vec::new(),
                continuation: opt(get(2)),
                more: opt(get(3)),
            }),
            "tile" => {
                let (Some(shelf), Some(kind)) = (page.shelves.last_mut(), kind_from(get(1))) else {
                    continue;
                };
                if get(2).is_empty() {
                    continue;
                }
                shelf.tiles.push(Tile {
                    kind,
                    id: get(2).to_string(),
                    title: get(3).to_string(),
                    byline: get(4).to_string(),
                    channel_id: get(5).to_string(),
                    meta: get(6).to_string(),
                    badge: get(7).to_string(),
                    thumb: get(8).to_string(),
                    progress: None,
                });
            }
            _ => {}
        }
    }
    page.shelves.retain(|s| !s.tiles.is_empty());
    (!page.shelves.is_empty()).then_some(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> Page {
        Page {
            title: String::new(),
            shelves: vec![
                Shelf {
                    title: "Live\tnow".into(),
                    tiles: vec![Tile {
                        kind: Kind::Live,
                        id: "aaaaaaaaaaa".into(),
                        title: "Night\nsky".into(),
                        byline: "Some channel".into(),
                        channel_id: "UCx".into(),
                        meta: "1K watching".into(),
                        badge: "LIVE".into(),
                        thumb: "https://i.ytimg.com/vi/aaaaaaaaaaa/hqdefault.jpg".into(),
                        progress: Some(0.5),
                    }],
                    continuation: Some("tok".into()),
                    more: None,
                },
                Shelf {
                    title: "Channels".into(),
                    tiles: vec![Tile {
                        kind: Kind::Channel,
                        id: "UCy".into(),
                        title: "A channel".into(),
                        ..Tile::default()
                    }],
                    continuation: None,
                    more: Some("UCy".into()),
                },
            ],
            continuation: None,
        }
    }

    #[test]
    fn home_round_trips_for_the_same_context_only() {
        let text = to_text("ctx-pt-BR", &home());
        let back = parse("ctx-pt-BR", &text).expect("home");
        assert_eq!(back.shelves.len(), 2);
        let (live, channels) = (&back.shelves[0], &back.shelves[1]);
        // Tabs and line breaks inside fields become spaces.
        assert_eq!(live.title, "Live now");
        assert_eq!(live.tiles[0].title, "Night sky");
        assert_eq!(live.tiles[0].kind, Kind::Live);
        assert_eq!(live.tiles[0].badge, "LIVE");
        assert_eq!(live.tiles[0].progress, None);
        assert_eq!(live.continuation.as_deref(), Some("tok"));
        assert_eq!(channels.tiles[0].kind, Kind::Channel);
        assert_eq!(channels.more.as_deref(), Some("UCy"));
        assert!(parse("ctx-en-US", &text).is_none());
        assert!(parse("ctx-pt-BR", "something else").is_none());
        assert!(parse("ctx-pt-BR", &format!("{HEADER}\nctx\tctx-pt-BR\n")).is_none());
    }
}
