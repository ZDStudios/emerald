//! Window/app icon from the embedded PNG.
use cef::*;

pub fn window_icon() -> Option<Image> {
    let png = crate::scheme::asset("img/emerald-256.png")?;
    let img = image_create()?;
    (img.add_png(1.0, Some(&png)) != 0).then_some(img)
}
