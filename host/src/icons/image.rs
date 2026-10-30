//! Decode untrusted input once, strip metadata, and produce a bounded PNG for every client.
use anyhow::{Context, Result, bail, ensure};
use image::{DynamicImage, ImageDecoder as _, ImageFormat, ImageReader, Limits};
use std::io::Cursor;

pub(super) const MAX_INPUT: usize = 5 * 1024 * 1024;
pub(super) const MAX_PNG: usize = 48 * 1024;
const MAX_SIDE: u32 = 4096;
const ICON_SIDE: u32 = 256;
const MAX_SVG: usize = 256 * 1024;

pub(super) fn normalize(bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(!bytes.is_empty() && bytes.len() <= MAX_INPUT, "Choose an image up to 5 MiB");
    let image = match image::guess_format(bytes) {
        Ok(format @ (ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)) => {
            let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
            let mut limits = Limits::default();
            limits.max_image_width = Some(MAX_SIDE);
            limits.max_image_height = Some(MAX_SIDE);
            limits.max_alloc = Some(128 * 1024 * 1024);
            reader.limits(limits);
            let mut decoder = reader.into_decoder().context("Image is damaged or exceeds 4096 × 4096 pixels")?;
            let orientation = decoder.orientation()?;
            let mut image = DynamicImage::from_decoder(decoder).context("Cannot decode image")?;
            image.apply_orientation(orientation);
            image
        }
        Ok(_) => bail!("Choose PNG, JPEG, WebP or SVG"),
        Err(_) => svg(bytes)?,
    };
    let mut side = ICON_SIDE;
    loop {
        let resized = image.resize(side, side, image::imageops::FilterType::Lanczos3);
        let mut out = Cursor::new(Vec::new());
        resized.write_to(&mut out, ImageFormat::Png)?;
        let out = out.into_inner();
        if out.len() <= MAX_PNG {
            return Ok(out);
        }
        ensure!(side > 32, "Image could not be compressed");
        side /= 2;
    }
}

fn svg(bytes: &[u8]) -> Result<DynamicImage> {
    ensure!(bytes.len() <= MAX_SVG, "SVG images must be at most 256 KiB");
    // No filesystem resources, embedded images, fonts, scripts or network resources.
    let options = resvg::usvg::Options {
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(bytes, &options).context("Choose a valid PNG, JPEG, WebP or SVG image")?;
    let size = tree.size();
    ensure!(size.width() <= 4096.0 && size.height() <= 4096.0, "SVG dimensions exceed 4096 × 4096 pixels");
    let scale = 256.0 / size.width().max(size.height());
    let pixels = size.scale_by(scale).context("Invalid SVG dimensions")?.to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pixels.width(), pixels.height()).context("Cannot render SVG")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia holds premultiplied RGBA; encoding PNG restores straight alpha.
    Ok(image::load_from_memory_with_format(&pixmap.encode_png()?, ImageFormat::Png)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_is_rasterized_without_external_images() {
        let png = normalize(br##"<svg xmlns="http://www.w3.org/2000/svg" width="512" height="256"><rect width="512" height="256" fill="#ff0000"/><image href="file:///etc/passwd" width="100" height="100"/></svg>"##).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (256, 128));
        assert!(png.len() <= MAX_PNG);
    }

    #[test]
    fn oversized_and_broken_images_are_rejected() {
        assert!(normalize(&[]).is_err());
        assert!(normalize(&vec![0; MAX_INPUT + 1]).is_err());
        assert!(normalize(b"not an image").is_err());
        assert!(normalize(br#"<svg xmlns="http://www.w3.org/2000/svg" width="9000" height="9000"/>"#).is_err());
        let huge = DynamicImage::new_rgba8(4097, 1);
        let mut png = Cursor::new(Vec::new());
        huge.write_to(&mut png, ImageFormat::Png).unwrap();
        assert!(normalize(png.get_ref()).is_err());
    }
}
