//! Background image decoding and a bounded in-memory thumbnail cache.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 512;
const DECODE_LIMIT: u32 = 4096;
const MISSING_RETRY: Duration = Duration::from_secs(30);

enum Picture {
    Decoded(ColorImage),
    Texture(TextureHandle),
    Missing(Instant),
}

struct Entry {
    picture: Picture,
    bytes: usize,
    last_used: u64,
}

pub struct Images {
    entries: RefCell<HashMap<String, Entry>>,
    wanted: RefCell<Vec<(String, bool)>>,
    bytes: usize,
    budget: usize,
}

impl Images {
    pub fn new(low_power: bool) -> Self {
        Self {
            entries: RefCell::new(HashMap::new()),
            wanted: RefCell::new(Vec::new()),
            bytes: 0,
            budget: if low_power { 16 } else { 64 } * 1024 * 1024,
        }
    }

    pub fn get(&self, ctx: &egui::Context, url: &str, priority: bool) -> Option<egui::TextureId> {
        if url.is_empty() {
            return None;
        }
        let mut retry_expired = false;
        {
            let mut entries = self.entries.borrow_mut();
            if let Some(entry) = entries.get_mut(url) {
                match &entry.picture {
                    Picture::Missing(retry_at) => {
                        if Instant::now() < *retry_at {
                            return None;
                        }
                        retry_expired = true;
                    }
                    Picture::Texture(texture) => {
                        entry.last_used = ctx.cumulative_frame_nr();
                        return Some(texture.id());
                    }
                    Picture::Decoded(_) => {
                        entry.last_used = ctx.cumulative_frame_nr();
                        let Picture::Decoded(image) = std::mem::replace(
                            &mut entry.picture,
                            Picture::Missing(Instant::now() + MISSING_RETRY),
                        ) else {
                            unreachable!();
                        };
                        let texture = ctx.load_texture(url, image, TextureOptions::LINEAR);
                        let id = texture.id();
                        entry.picture = Picture::Texture(texture);
                        return Some(id);
                    }
                }
            }
            if retry_expired {
                entries.remove(url);
            }
        }
        let mut wanted = self.wanted.borrow_mut();
        if let Some((_, requested_priority)) = wanted.iter_mut().find(|(key, _)| key == url) {
            *requested_priority |= priority;
        } else {
            wanted.push((url.to_owned(), priority));
        }
        None
    }

    pub fn size(&self, url: &str) -> Option<[usize; 2]> {
        match &self.entries.borrow().get(url)?.picture {
            Picture::Texture(texture) => Some(texture.size()),
            _ => None,
        }
    }

    /// Only requests made while drawing the current frame. Focused cards go first.
    pub fn take_wanted(&mut self) -> impl Iterator<Item = String> + use<> {
        let mut wanted = std::mem::take(self.wanted.get_mut());
        wanted.sort_by_key(|(_, priority)| !priority);
        wanted.into_iter().map(|(url, _)| url)
    }

    pub fn complete(&mut self, url: String, image: Option<ColorImage>, frame: u64) {
        self.complete_at(url, image, frame, Instant::now() + MISSING_RETRY);
    }

    fn complete_at(
        &mut self,
        url: String,
        image: Option<ColorImage>,
        frame: u64,
        retry_at: Instant,
    ) {
        let bytes = image.as_ref().map_or(0, |image| image.pixels.len() * 4);
        let entry = Entry {
            picture: image.map_or(Picture::Missing(retry_at), Picture::Decoded),
            bytes,
            last_used: frame,
        };
        let previous = self.entries.get_mut().insert(url, entry);
        self.bytes += bytes;
        if let Some(previous) = previous {
            self.bytes -= previous.bytes;
        }
    }

    /// Run after drawing so textures touched in this frame are newest.
    pub fn trim(&mut self) {
        let entries = self.entries.get_mut();
        if self.bytes <= self.budget && entries.len() <= MAX_ENTRIES {
            return;
        }
        let mut oldest: Vec<_> = entries
            .iter()
            .map(|(url, entry)| (entry.last_used, url.clone()))
            .collect();
        oldest.sort_unstable();
        for (_, url) in oldest {
            if self.bytes <= self.budget && entries.len() <= MAX_ENTRIES {
                break;
            }
            self.bytes -= entries.remove(&url).expect("cached picture").bytes;
        }
    }
}

/// Decode a downloaded PNG, JPEG or WebP away from the interface thread.
pub fn decode(path: &str, max_side: u32) -> Option<ColorImage> {
    let mut reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(DECODE_LIMIT);
    limits.max_image_height = Some(DECODE_LIMIT);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let (w, h, px) = shrink(w, h, img.into_raw(), max_side);
    Some(ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        &px,
    ))
}

/// Scale RGBA pixels down by a whole factor (box average) until both
/// sides fit in `max`. Pictures that already fit are returned as they are.
pub fn shrink(w: u32, h: u32, px: Vec<u8>, max: u32) -> (u32, u32, Vec<u8>) {
    let k = w.max(h).div_ceil(max.max(1));
    if k <= 1 {
        return (w, h, px);
    }
    let (nw, nh) = ((w / k).max(1), (h / k).max(1));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        let sy0 = (y * k).min(h - 1);
        let sy1 = (sy0 + k).min(h);
        for x in 0..nw {
            let sx0 = (x * k).min(w - 1);
            let sx1 = (sx0 + k).min(w);
            let mut acc = [0u32; 4];
            let mut count = 0u32;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let i = ((sy * w + sx) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += px[i + c] as u32;
                    }
                    count += 1;
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            let div = count.max(1);
            for c in 0..4 {
                out[o + c] = (acc[c] / div) as u8;
            }
        }
    }
    (nw, nh, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrink_box_averages_and_keeps_small_pictures() {
        let px = vec![10u8; 4 * 4 * 4];
        assert_eq!(shrink(4, 4, px.clone(), 8), (4, 4, px));
        let mut px = vec![0u8; 4 * 2 * 4];
        // Left 2×2 block white, right block black.
        for y in 0..2 {
            for x in 0..2 {
                let i = (y * 4 + x) * 4;
                px[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let (w, h, out) = shrink(4, 2, px, 2);
        assert_eq!((w, h), (2, 1));
        assert_eq!(&out[0..4], &[255, 255, 255, 255]);
        assert_eq!(&out[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn focus_has_priority_and_least_recently_used_images_are_evicted() {
        let ctx = egui::Context::default();
        let mut images = Images::new(true);
        images.budget = 8;
        images.get(&ctx, "first", false);
        images.get(&ctx, "focused", false);
        images.get(&ctx, "focused", true);
        assert_eq!(
            images.take_wanted().collect::<Vec<_>>(),
            ["focused", "first"]
        );

        for (frame, name) in ["older", "recent", "new"].into_iter().enumerate() {
            let one_pixel = ColorImage::filled([1, 1], egui::Color32::WHITE);
            images.complete(name.into(), Some(one_pixel), frame as u64);
        }
        images.trim();
        assert!(!images.entries.borrow().contains_key("older"));
        assert!(images.get(&ctx, "recent", false).is_some());
        assert!(images.get(&ctx, "new", false).is_some());
        assert_eq!(images.bytes, 8);
    }

    #[test]
    fn shrink_handles_extreme_aspect_ratios_without_panicking() {
        // 1x4 shrunk to max 2 => k=2, nw=1, nh=2: must stay in bounds.
        let mut px = Vec::new();
        for v in [10u8, 30, 100, 200] {
            px.extend_from_slice(&[v, v, v, 255]);
        }
        let (w, h, out) = shrink(1, 4, px, 2);
        assert_eq!((w, h), (1, 2));
        assert_eq!(&out[0..4], &[20, 20, 20, 255]);
        assert_eq!(&out[4..8], &[150, 150, 150, 255]);
        // 4x1 shrunk to max 2 => k=2, nw=2, nh=1.
        let (w, h, out) = shrink(4, 1, vec![80u8; 4 * 4], 2);
        assert_eq!((w, h), (2, 1));
        assert_eq!(out, vec![80u8; 2 * 4]);
    }

    #[test]
    fn missing_images_retry_after_cooldown() {
        let ctx = egui::Context::default();
        let mut images = Images::new(true);
        // Active cooldown suppresses immediate retries.
        images.complete_at(
            "https://i.ytimg.com/vi/a/hqdefault.jpg".into(),
            None,
            1,
            Instant::now() + Duration::from_secs(60),
        );
        assert!(
            images
                .get(&ctx, "https://i.ytimg.com/vi/a/hqdefault.jpg", false)
                .is_none()
        );
        assert_eq!(images.take_wanted().count(), 0);
        // Expired cooldown re-queues the URL on the next draw.
        images.complete_at(
            "https://i.ytimg.com/vi/a/hqdefault.jpg".into(),
            None,
            1,
            Instant::now() - Duration::from_secs(1),
        );
        assert!(
            images
                .get(&ctx, "https://i.ytimg.com/vi/a/hqdefault.jpg", true)
                .is_none()
        );
        assert_eq!(
            images.take_wanted().collect::<Vec<_>>(),
            ["https://i.ytimg.com/vi/a/hqdefault.jpg"]
        );
    }
}
