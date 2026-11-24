//! The app's name and logo, fixed at build time (see `build.rs`):
//! `CATTUBE_NAME` and `CATTUBE_LOGO` brand a build made for yourself.

use eframe::egui::{self, TextureHandle, TextureOptions};

/// The name shown in the header, Settings and the window title.
pub const NAME: &str = env!("CATTUBE_APP_NAME");

/// The PNG given as `CATTUBE_LOGO`; empty for CatTube's own mark.
const LOGO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo.png"));

/// The build's logo as a texture and its size in pixels, decoded once;
/// `None` when the build has no logo (or it does not decode).
pub fn logo(ctx: &egui::Context) -> Option<(egui::TextureId, [f32; 2])> {
    if LOGO.is_empty() {
        return None;
    }
    let key = egui::Id::new("cattube-brand-logo");
    if let Some(h) = ctx.data_mut(|d| d.get_temp::<Option<TextureHandle>>(key)) {
        return h.map(|h| (h.id(), h.size_vec2().into()));
    }
    let handle = image::load_from_memory_with_format(LOGO, image::ImageFormat::Png)
        .ok()
        .map(|img| {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            ctx.load_texture("brand-logo", image, TextureOptions::LINEAR)
        });
    ctx.data_mut(|d| d.insert_temp(key, handle.clone()));
    handle.map(|h| (h.id(), h.size_vec2().into()))
}
