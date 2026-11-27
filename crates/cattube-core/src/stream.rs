//! Pure stream-selection, URL validation, EDL assembly, and chapter
//! metadata formatting for direct playback without mpv's `ytdl_hook`.

use crate::mpv::{Json, is_playable_url, option_value, parse_json};
use crate::player::Codec;
use crate::youtube::is_video_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamSource {
    #[default]
    Ytdl,
    Innertube,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoFormat {
    pub url: String,
    pub height: u32,
    pub fps: f64,
    pub vcodec: String,
    pub acodec: String,
    pub hdr: bool,
    pub protocol: String,
    pub tbr: f64,
    pub format_id: String,
    pub available_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFormat {
    pub url: String,
    pub acodec: String,
    pub abr: f64,
    pub language: String,
    pub title: String,
    pub is_default: bool,
    pub protocol: String,
    pub available_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleTrack {
    pub lang: String,
    pub name: String,
    pub url: String,
    pub auto: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub start_time: f64,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedStream {
    pub video_id: String,
    pub title: String,
    pub is_live: bool,
    pub duration: Option<f64>,
    pub user_agent: String,
    pub headers: Vec<(String, String)>,
    pub videos: Vec<VideoFormat>,
    pub audios: Vec<AudioFormat>,
    pub subtitles: Vec<SubtitleTrack>,
    pub chapters: Vec<Chapter>,
    pub source: StreamSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectedPlayback {
    pub video_url: String,
    pub audio_url: Option<String>,
    pub extra_audios: Vec<AudioFormat>,
    pub is_live: bool,
    pub height: u32,
    pub available_at: Option<u64>,
    pub user_agent: String,
    pub headers: Vec<(String, String)>,
}

fn clean_header_part(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

fn json_u32(v: Option<&Json>) -> u32 {
    v.and_then(Json::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map_or(0, |n| n.round() as u32)
}

fn json_u64(v: Option<&Json>) -> Option<u64> {
    v.and_then(Json::as_f64)
        .filter(|n| n.is_finite() && *n > 0.0)
        .map(|n| n as u64)
}

fn json_f64(v: Option<&Json>) -> f64 {
    v.and_then(Json::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.0)
        .unwrap_or(0.0)
}

fn json_str(v: Option<&Json>) -> String {
    v.and_then(Json::as_str).unwrap_or("").to_string()
}

/// Parse a `ytdl-server.py` `mode == "resolve"` JSON response (either the full
/// `{"ok": true, "stream": {...}}` envelope or the inner stream object).
pub fn parse_resolved_stream(json_text: &str) -> Option<ResolvedStream> {
    let root = parse_json(json_text.trim())?;
    if root.get("ok").and_then(Json::as_bool) == Some(false) {
        return None;
    }
    let obj = root.get("stream").unwrap_or(&root);
    let video_id = obj.get("video_id").and_then(Json::as_str)?;
    if !is_video_id(video_id) {
        return None;
    }
    let title = json_str(obj.get("title"));
    let is_live = obj.get("is_live").and_then(Json::as_bool).unwrap_or(false);
    let duration = obj
        .get("duration")
        .and_then(Json::as_f64)
        .filter(|d| d.is_finite() && *d > 0.0);
    let user_agent = clean_header_part(&json_str(obj.get("user_agent")));

    let mut headers = Vec::new();
    if let Some(Json::Obj(kv)) = obj.get("headers") {
        for (k, v) in kv {
            if let Some(val) = v.as_str() {
                let k = clean_header_part(k);
                let val = clean_header_part(val);
                if !k.is_empty() && !val.is_empty() {
                    headers.push((k, val));
                }
            }
        }
    }

    let mut videos = Vec::new();
    for v in obj.get("videos").and_then(Json::as_arr).unwrap_or(&[]) {
        let url = json_str(v.get("url"));
        if !is_playable_url(&url) {
            continue;
        }
        videos.push(VideoFormat {
            url,
            height: json_u32(v.get("height")),
            fps: json_f64(v.get("fps")),
            vcodec: json_str(v.get("vcodec")),
            acodec: json_str(v.get("acodec")),
            hdr: v.get("hdr").and_then(Json::as_bool).unwrap_or(false),
            protocol: json_str(v.get("protocol")),
            tbr: json_f64(v.get("tbr")),
            format_id: json_str(v.get("format_id")),
            available_at: json_u64(v.get("available_at")),
        });
    }
    if videos.is_empty() {
        return None;
    }

    let mut audios = Vec::new();
    for a in obj.get("audios").and_then(Json::as_arr).unwrap_or(&[]) {
        let url = json_str(a.get("url"));
        if !is_playable_url(&url) {
            continue;
        }
        audios.push(AudioFormat {
            url,
            acodec: json_str(a.get("acodec")),
            abr: json_f64(a.get("abr")),
            language: json_str(a.get("language")),
            title: json_str(a.get("title")),
            is_default: a.get("is_default").and_then(Json::as_bool).unwrap_or(false),
            protocol: json_str(a.get("protocol")),
            available_at: json_u64(a.get("available_at")),
        });
    }

    let mut subtitles = Vec::new();
    for s in obj.get("subtitles").and_then(Json::as_arr).unwrap_or(&[]) {
        let url = json_str(s.get("url"));
        let lang = json_str(s.get("lang"));
        if !is_playable_url(&url) || lang.is_empty() {
            continue;
        }
        subtitles.push(SubtitleTrack {
            lang,
            name: json_str(s.get("name")),
            url,
            auto: s.get("auto").and_then(Json::as_bool).unwrap_or(false),
        });
    }

    let mut chapters = Vec::new();
    for c in obj.get("chapters").and_then(Json::as_arr).unwrap_or(&[]) {
        let Some(start_time) = c
            .get("start_time")
            .and_then(Json::as_f64)
            .filter(|t| t.is_finite() && *t >= 0.0)
        else {
            continue;
        };
        let ctitle = json_str(c.get("title"));
        if !ctitle.trim().is_empty() {
            chapters.push(Chapter {
                start_time,
                title: ctitle,
            });
        }
    }

    Some(ResolvedStream {
        video_id: video_id.to_string(),
        title,
        is_live,
        duration,
        user_agent,
        headers,
        videos,
        audios,
        subtitles,
        chapters,
        source: StreamSource::Ytdl,
    })
}

fn codec_matches(vcodec: &str, codec: Codec) -> bool {
    match codec {
        Codec::Av1 => vcodec.starts_with("av01"),
        Codec::Vp9 => vcodec.starts_with("vp09") || vcodec == "vp9",
        Codec::Avc => vcodec.starts_with("avc1"),
    }
}

fn is_av1(vcodec: &str) -> bool {
    vcodec.starts_with("av01")
}

fn is_muxed(v: &VideoFormat) -> bool {
    !v.acodec.is_empty() && v.acodec != "none"
}

fn best_audio_for(audios: &[AudioFormat]) -> Option<&AudioFormat> {
    audios.iter().max_by(|a, b| {
        a.is_default
            .cmp(&b.is_default)
            .then_with(|| {
                let pa = audio_codec_rank(&a.acodec);
                let pb = audio_codec_rank(&b.acodec);
                pa.cmp(&pb)
            })
            .then_with(|| a.abr.total_cmp(&b.abr))
    })
}

fn audio_codec_rank(acodec: &str) -> u8 {
    if acodec.contains("opus") {
        2
    } else if acodec.starts_with("mp4a") {
        1
    } else {
        0
    }
}

fn pick_best<F>(
    videos: &[VideoFormat],
    has_audio: bool,
    is_live: bool,
    hdr_pref: bool,
    pref_codec: Codec,
    pred: F,
) -> Option<&VideoFormat>
where
    F: Fn(&VideoFormat) -> bool,
{
    videos
        .iter()
        .filter(|v| {
            let playable_audio = has_audio || is_muxed(v);
            playable_audio && pred(v)
        })
        .max_by(|a, b| {
            // For live streams, prefer already-muxed streams (e.g., HLS) so audio stays synchronized.
            let live_mux_a = u8::from(is_live && is_muxed(a));
            let live_mux_b = u8::from(is_live && is_muxed(b));
            // For VOD with separate audio, prefer video-only adaptive streams over legacy muxed streams.
            let vod_sep_a = u8::from(!is_live && has_audio && !is_muxed(a));
            let vod_sep_b = u8::from(!is_live && has_audio && !is_muxed(b));
            live_mux_a
                .cmp(&live_mux_b)
                .then_with(|| vod_sep_a.cmp(&vod_sep_b))
                .then_with(|| a.height.cmp(&b.height))
                .then_with(|| {
                    u8::from(codec_matches(&a.vcodec, pref_codec))
                        .cmp(&u8::from(codec_matches(&b.vcodec, pref_codec)))
                })
                .then_with(|| u8::from(hdr_pref && a.hdr).cmp(&u8::from(hdr_pref && b.hdr)))
                .then_with(|| a.fps.total_cmp(&b.fps))
                .then_with(|| a.tbr.total_cmp(&b.tbr))
        })
}

/// Select the best matching video + audio pair for the active playback preferences,
/// following the same quality/codec/low-power rules as [`crate::player::format_string`].
pub fn select_stream(
    resolved: &ResolvedStream,
    max_height: u32,
    codec: Codec,
    hdr: bool,
    low_power: bool,
) -> Option<SelectedPlayback> {
    if resolved.videos.is_empty() {
        return None;
    }
    let primary_audio = if resolved.is_live {
        None
    } else {
        best_audio_for(&resolved.audios)
    };
    let has_audio = primary_audio.is_some();
    let height_ok = |v: &VideoFormat| max_height == 0 || (v.height > 0 && v.height <= max_height);
    let sdr_ok = |v: &VideoFormat| hdr || !v.hdr;
    let fps30 = |v: &VideoFormat| v.fps <= 0.0 || v.fps <= 30.5;
    let fps60 = |v: &VideoFormat| v.fps <= 0.0 || v.fps <= 60.5;

    let mut chosen: Option<&VideoFormat> = None;
    if low_power {
        match codec {
            Codec::Avc => {
                if max_height > 720 {
                    chosen = chosen.or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| {
                                v.height > 720
                                    && v.height <= max_height
                                    && sdr_ok(v)
                                    && fps30(v)
                                    && codec_matches(&v.vcodec, Codec::Avc)
                            },
                        )
                    });
                    chosen = chosen.or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| {
                                v.height > 0
                                    && v.height <= 720
                                    && sdr_ok(v)
                                    && fps60(v)
                                    && codec_matches(&v.vcodec, Codec::Avc)
                            },
                        )
                    });
                } else if max_height > 0 {
                    chosen = chosen.or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| {
                                height_ok(v)
                                    && sdr_ok(v)
                                    && fps60(v)
                                    && codec_matches(&v.vcodec, Codec::Avc)
                            },
                        )
                    });
                } else {
                    chosen = chosen.or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| sdr_ok(v) && fps30(v) && codec_matches(&v.vcodec, Codec::Avc),
                        )
                    });
                }
                chosen = chosen
                    .or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| height_ok(v) && sdr_ok(v) && fps30(v) && !is_av1(&v.vcodec),
                        )
                    })
                    .or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| height_ok(v) && sdr_ok(v) && !is_av1(&v.vcodec),
                        )
                    })
                    .or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| height_ok(v) && sdr_ok(v) && fps30(v),
                        )
                    });
            }
            Codec::Vp9 => {
                chosen = chosen
                    .or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| {
                                height_ok(v)
                                    && sdr_ok(v)
                                    && fps30(v)
                                    && codec_matches(&v.vcodec, Codec::Vp9)
                            },
                        )
                    })
                    .or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| height_ok(v) && sdr_ok(v) && fps30(v) && !is_av1(&v.vcodec),
                        )
                    })
                    .or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| height_ok(v) && sdr_ok(v) && !is_av1(&v.vcodec),
                        )
                    })
                    .or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| height_ok(v) && sdr_ok(v) && fps30(v),
                        )
                    });
            }
            Codec::Av1 => {
                chosen = chosen
                    .or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| {
                                height_ok(v)
                                    && sdr_ok(v)
                                    && fps30(v)
                                    && codec_matches(&v.vcodec, Codec::Av1)
                            },
                        )
                    })
                    .or_else(|| {
                        pick_best(
                            &resolved.videos,
                            has_audio,
                            resolved.is_live,
                            hdr,
                            codec,
                            |v| height_ok(v) && sdr_ok(v) && fps30(v),
                        )
                    });
            }
        }
    }

    // General / fallback tiers.
    chosen = chosen
        .or_else(|| {
            pick_best(
                &resolved.videos,
                has_audio,
                resolved.is_live,
                hdr,
                codec,
                |v| height_ok(v) && sdr_ok(v) && codec_matches(&v.vcodec, codec),
            )
        })
        .or_else(|| {
            pick_best(
                &resolved.videos,
                has_audio,
                resolved.is_live,
                hdr,
                codec,
                |v| height_ok(v) && sdr_ok(v),
            )
        })
        .or_else(|| {
            pick_best(
                &resolved.videos,
                has_audio,
                resolved.is_live,
                hdr,
                codec,
                height_ok,
            )
        })
        .or_else(|| {
            pick_best(
                &resolved.videos,
                has_audio,
                resolved.is_live,
                hdr,
                codec,
                |_| true,
            )
        })
        .or_else(|| resolved.videos.first());

    let video = chosen?;
    let muxed = is_muxed(video);
    let audio = if resolved.is_live || muxed {
        None
    } else {
        primary_audio
    };

    let mut extra_audios = Vec::new();
    if let Some(main_audio) = audio {
        let mut seen: Vec<(String, String)> = vec![(
            main_audio.language.to_ascii_lowercase(),
            main_audio.title.to_ascii_lowercase(),
        )];
        for a in &resolved.audios {
            if a.url == main_audio.url {
                continue;
            }
            let key = (
                a.language.to_ascii_lowercase(),
                a.title.to_ascii_lowercase(),
            );
            if key == (String::new(), String::new()) || seen.contains(&key) {
                continue;
            }
            seen.push(key);
            extra_audios.push(a.clone());
        }
    }

    let available_at = match (video.available_at, audio.and_then(|a| a.available_at)) {
        (Some(va), Some(aa)) => Some(va.max(aa)),
        (Some(va), None) => Some(va),
        (None, Some(aa)) => Some(aa),
        (None, None) => None,
    };

    Some(SelectedPlayback {
        video_url: video.url.clone(),
        audio_url: audio.map(|a| a.url.clone()),
        extra_audios,
        is_live: resolved.is_live,
        height: video.height,
        available_at,
        user_agent: resolved.user_agent.clone(),
        headers: resolved.headers.clone(),
    })
}

/// Build an mpv playback URI for `video_url` and optional separate `audio_url`.
/// Separate streams use mpv's length-prefixed `edl://` syntax so special URL
/// characters (commas, semicolons, percent signs) are never misinterpreted.
pub fn edl_uri(video_url: &str, audio_url: Option<&str>) -> String {
    match audio_url {
        None => video_url.to_string(),
        Some(audio) => format!(
            "edl://!no_clip;!no_chapters;{};!new_stream;!no_clip;!no_chapters;{}",
            option_value(video_url),
            option_value(audio)
        ),
    }
}

fn escape_ffmetadata(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '=' | ';' | '#' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\\n"),
            '\r' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Format video chapters as an FFmpeg `;FFMETADATA1` file for `chapters-file=`.
pub fn ffmetadata_chapters(chapters: &[Chapter], duration: Option<f64>) -> String {
    let mut out = String::from(";FFMETADATA1\n");
    for (i, ch) in chapters.iter().enumerate() {
        let start = ch.start_time.max(0.0);
        let end = chapters
            .get(i + 1)
            .map(|next| next.start_time.max(start))
            .or_else(|| duration.filter(|d| d.is_finite() && *d > start))
            .unwrap_or(start + 1.0);
        let start_ms = (start * 1000.0).round() as u64;
        let end_ms = (end.max(start) * 1000.0).round() as u64;
        out.push_str(&format!(
            "[CHAPTER]\nTIMEBASE=1/1000\nSTART={start_ms}\nEND={end_ms}\ntitle={}\n",
            escape_ffmetadata(&ch.title)
        ));
    }
    out
}

/// Build per-file options for `mpv` `loadfile` when loading a resolved stream
/// directly (`ytdl=no`).
pub fn loadfile_options(
    start: f64,
    is_live: bool,
    title: &str,
    user_agent: &str,
    chapters_file: Option<&str>,
    headers: &[(String, String)],
) -> String {
    let mut opts = vec!["ytdl=no".to_string()];
    if !is_live && start.is_finite() && start > 0.0 {
        opts.push(format!("start={start:.3}"));
    }
    if !title.trim().is_empty() {
        opts.push(format!("force-media-title={}", option_value(title)));
    }
    if !user_agent.is_empty() {
        opts.push(format!("user-agent={}", option_value(user_agent)));
    }
    if let Some(cf) = chapters_file
        && !cf.is_empty()
    {
        opts.push(format!("chapters-file={}", option_value(cf)));
    }
    let extra_headers: Vec<String> = headers
        .iter()
        .filter(|(k, v)| {
            !k.eq_ignore_ascii_case("user-agent")
                && !k.is_empty()
                && !v.is_empty()
                && !k.chars().any(char::is_control)
                && !v.chars().any(char::is_control)
        })
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    if !extra_headers.is_empty() {
        opts.push(format!(
            "http-header-fields={}",
            option_value(&extra_headers.join(","))
        ));
    }
    opts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_resolved(live: bool) -> ResolvedStream {
        ResolvedStream {
            video_id: "WhWc3b3KhnY".into(),
            title: "Test, Video = 1".into(),
            is_live: live,
            duration: Some(300.0),
            user_agent: "Mozilla/5.0".into(),
            headers: vec![
                ("User-Agent".into(), "Mozilla/5.0".into()),
                ("Origin".into(), "https://www.youtube.com".into()),
            ],
            videos: vec![
                VideoFormat {
                    url: "https://cdn.example/18.mp4".into(),
                    height: 360,
                    fps: 30.0,
                    vcodec: "avc1.42001E".into(),
                    acodec: "mp4a.40.2".into(),
                    hdr: false,
                    protocol: "https".into(),
                    tbr: 500.0,
                    format_id: "18".into(),
                    available_at: None,
                },
                VideoFormat {
                    url: "https://cdn.example/avc720_60.mp4".into(),
                    height: 720,
                    fps: 60.0,
                    vcodec: "avc1.640020".into(),
                    acodec: "none".into(),
                    hdr: false,
                    protocol: "https".into(),
                    tbr: 2200.0,
                    format_id: "298".into(),
                    available_at: Some(100),
                },
                VideoFormat {
                    url: "https://cdn.example/avc1080_30.mp4".into(),
                    height: 1080,
                    fps: 30.0,
                    vcodec: "avc1.640028".into(),
                    acodec: "none".into(),
                    hdr: false,
                    protocol: "https".into(),
                    tbr: 3200.0,
                    format_id: "137".into(),
                    available_at: None,
                },
                VideoFormat {
                    url: "https://cdn.example/vp9_1080_60.webm".into(),
                    height: 1080,
                    fps: 60.0,
                    vcodec: "vp09.00.41.08".into(),
                    acodec: "none".into(),
                    hdr: false,
                    protocol: "https".into(),
                    tbr: 3600.0,
                    format_id: "303".into(),
                    available_at: None,
                },
                VideoFormat {
                    url: "https://cdn.example/vp9_1080_30_hdr.webm".into(),
                    height: 1080,
                    fps: 30.0,
                    vcodec: "vp09.02.41.10".into(),
                    acodec: "none".into(),
                    hdr: true,
                    protocol: "https".into(),
                    tbr: 4500.0,
                    format_id: "335".into(),
                    available_at: None,
                },
                VideoFormat {
                    url: "https://cdn.example/av1_720_30.mp4".into(),
                    height: 720,
                    fps: 30.0,
                    vcodec: "av01.0.05M.08".into(),
                    acodec: "none".into(),
                    hdr: false,
                    protocol: "https".into(),
                    tbr: 1500.0,
                    format_id: "398".into(),
                    available_at: None,
                },
            ],
            audios: vec![
                AudioFormat {
                    url: "https://cdn.example/audio_en.webm".into(),
                    acodec: "opus".into(),
                    abr: 128.0,
                    language: "en".into(),
                    title: "English".into(),
                    is_default: true,
                    protocol: "https".into(),
                    available_at: Some(120),
                },
                AudioFormat {
                    url: "https://cdn.example/audio_pt.m4a".into(),
                    acodec: "mp4a.40.2".into(),
                    abr: 128.0,
                    language: "pt".into(),
                    title: "Portuguese".into(),
                    is_default: false,
                    protocol: "https".into(),
                    available_at: None,
                },
            ],
            subtitles: vec![],
            chapters: vec![],
            source: StreamSource::Ytdl,
        }
    }

    #[test]
    fn parse_resolved_stream_validates_urls_and_fields() {
        let raw = r#"{
            "ok": true,
            "stream": {
                "video_id": "WhWc3b3KhnY",
                "title": "Spring",
                "is_live": false,
                "duration": 465.5,
                "user_agent": "UA\nEvil",
                "headers": {"Referer": "https://www.youtube.com/"},
                "videos": [
                    {"url": "javascript:alert(1)", "height": 1080, "fps": 30, "vcodec": "avc1", "acodec": "none"},
                    {"url": "https://cdn.example/v.mp4", "height": 720, "fps": 30, "vcodec": "avc1.64001f", "acodec": "none", "hdr": false, "tbr": 1800, "format_id": "136", "available_at": 1700000000}
                ],
                "audios": [
                    {"url": "https://cdn.example/a.webm", "acodec": "opus", "abr": 128, "language": "en", "title": "English", "is_default": true}
                ],
                "subtitles": [
                    {"lang": "en", "name": "English", "url": "https://www.youtube.com/api/timedtext?v=1", "auto": false}
                ],
                "chapters": [
                    {"start_time": 0, "title": "Intro"},
                    {"start_time": 42.5, "title": "Main"}
                ]
            }
        }"#;
        let parsed = parse_resolved_stream(raw).unwrap();
        assert_eq!(parsed.video_id, "WhWc3b3KhnY");
        assert_eq!(parsed.user_agent, "UAEvil");
        assert_eq!(parsed.videos.len(), 1);
        assert_eq!(parsed.videos[0].available_at, Some(1700000000));
        assert_eq!(parsed.audios.len(), 1);
        assert_eq!(parsed.subtitles.len(), 1);
        assert_eq!(parsed.chapters.len(), 2);
        assert!(parse_resolved_stream(r#"{"ok":false,"error":"nope"}"#).is_none());
        assert!(parse_resolved_stream(r#"{"video_id":"bad"}"#).is_none());
    }

    #[test]
    fn select_stream_follows_low_power_and_standard_policies() {
        let s = sample_resolved(false);

        // Low-power AVC capped at 720p picks 720p60 AVC and opus audio, propagates max available_at.
        let lp720 = select_stream(&s, 720, Codec::Avc, false, true).unwrap();
        assert_eq!(lp720.video_url, "https://cdn.example/avc720_60.mp4");
        assert_eq!(
            lp720.audio_url.as_deref(),
            Some("https://cdn.example/audio_en.webm")
        );
        assert_eq!(lp720.available_at, Some(120));
        assert_eq!(lp720.extra_audios.len(), 1);
        assert_eq!(lp720.extra_audios[0].language, "pt");

        // Low-power AVC capped at 1080p prefers 1080p30 AVC over 720p60.
        let lp1080 = select_stream(&s, 1080, Codec::Avc, false, true).unwrap();
        assert_eq!(lp1080.video_url, "https://cdn.example/avc1080_30.mp4");

        // Low-power VP9 avoids 60fps and avoids AV1, falling back to 1080p30 AVC when VP9 30fps SDR is absent.
        let lp_vp9 = select_stream(&s, 1080, Codec::Vp9, false, true).unwrap();
        assert_eq!(lp_vp9.video_url, "https://cdn.example/avc1080_30.mp4");

        // Standard VP9 SDR 1080p picks 1080p60 VP9 (not HDR).
        let std_vp9 = select_stream(&s, 1080, Codec::Vp9, false, false).unwrap();
        assert_eq!(std_vp9.video_url, "https://cdn.example/vp9_1080_60.webm");

        // Standard VP9 with HDR enabled picks HDR VP9.
        let std_hdr = select_stream(&s, 1080, Codec::Vp9, true, false).unwrap();
        assert_eq!(
            std_hdr.video_url,
            "https://cdn.example/vp9_1080_30_hdr.webm"
        );

        // Explicit AV1 choice picks AV1.
        let av1 = select_stream(&s, 720, Codec::Av1, false, true).unwrap();
        assert_eq!(av1.video_url, "https://cdn.example/av1_720_30.mp4");
    }

    #[test]
    fn select_stream_omits_audio_for_live_and_muxed_streams() {
        let mut live = sample_resolved(true);
        live.videos = vec![VideoFormat {
            url: "https://cdn.example/live.m3u8".into(),
            height: 720,
            fps: 30.0,
            vcodec: "avc1".into(),
            acodec: "mp4a.40.2".into(),
            hdr: false,
            protocol: "m3u8_native".into(),
            tbr: 2000.0,
            format_id: "95".into(),
            available_at: None,
        }];
        let picked = select_stream(&live, 720, Codec::Avc, false, true).unwrap();
        assert!(picked.audio_url.is_none());
        assert!(picked.extra_audios.is_empty());
    }

    #[test]
    fn edl_ffmetadata_and_loadfile_options_quote_special_characters() {
        let v = "https://cdn.example/videoplayback?a=1,2;3%20x";
        let a = "https://cdn.example/audio?b=4;5";
        assert_eq!(edl_uri(v, None), v);
        let edl = edl_uri(v, Some(a));
        assert_eq!(
            edl,
            format!(
                "edl://!no_clip;!no_chapters;%{}%{v};!new_stream;!no_clip;!no_chapters;%{}%{a}",
                v.len(),
                a.len()
            )
        );

        let meta = ffmetadata_chapters(
            &[
                Chapter {
                    start_time: 0.0,
                    title: "Intro = 1; #test\\".into(),
                },
                Chapter {
                    start_time: 65.5,
                    title: "Part 2".into(),
                },
            ],
            Some(120.0),
        );
        assert!(meta.starts_with(";FFMETADATA1\n"));
        assert!(meta.contains("START=0\nEND=65500\ntitle=Intro \\= 1\\; \\#test\\\\\n"));
        assert!(meta.contains("START=65500\nEND=120000\ntitle=Part 2\n"));

        let opts = loadfile_options(
            42.5,
            false,
            "My, Title=1",
            "UA/1.0",
            Some("/tmp/ch,1.ffmeta"),
            &[
                ("User-Agent".into(), "Ignored".into()),
                ("Origin".into(), "https://www.youtube.com".into()),
            ],
        );
        assert!(opts.contains("ytdl=no"));
        assert!(opts.contains("start=42.500"));
        assert!(opts.contains("force-media-title=%11%My, Title=1"));
        assert!(opts.contains("user-agent=%6%UA/1.0"));
        assert!(opts.contains("chapters-file=%16%/tmp/ch,1.ffmeta"));
        assert!(opts.contains("http-header-fields=%31%Origin: https://www.youtube.com"));
        assert!(!opts.contains("Ignored"));

        let live_opts = loadfile_options(42.5, true, "", "", None, &[]);
        assert_eq!(live_opts, "ytdl=no");
    }
}
