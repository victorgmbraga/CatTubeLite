//! Background image decoding and a bounded in-memory thumbnail cache.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::cell::RefCell;
use std::collections::HashMap;

const MAX_ENTRIES: usize = 512;
const DECODE_LIMIT: u32 = 4096;

enum Picture {
    Decoded(ColorImage),
    Texture(TextureHandle),
    Missing,
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
        if let Some(entry) = self.entries.borrow_mut().get_mut(url) {
            entry.last_used = ctx.cumulative_frame_nr();
            return match &entry.picture {
                Picture::Missing => None,
                Picture::Texture(texture) => Some(texture.id()),
                Picture::Decoded(_) => {
                    let Picture::Decoded(image) =
                        std::mem::replace(&mut entry.picture, Picture::Missing)
                    else {
                        unreachable!();
                    };
                    let texture = ctx.load_texture(url, image, TextureOptions::LINEAR);
                    let id = texture.id();
                    entry.picture = Picture::Texture(texture);
                    Some(id)
                }
            };
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
        let bytes = image.as_ref().map_or(0, |image| image.pixels.len() * 4);
        let entry = Entry {
            picture: image.map_or(Picture::Missing, Picture::Decoded),
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
        for x in 0..nw {
            let mut acc = [0u32; 4];
            for dy in 0..k {
                for dx in 0..k {
                    let i = (((y * k + dy) * w + (x * k + dx)) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += px[i + c] as u32;
                    }
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (acc[c] / (k * k)) as u8;
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
}
