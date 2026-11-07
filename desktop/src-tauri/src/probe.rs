//! What LINE's own app works out about a photo or video before sending it:
//! the picture's size and thumbnail, and a video's length and first-frame
//! thumbnail.
//! Read on macOS with ImageIO and AVFoundation, without ffmpeg. Anything
//! that can't be read is left out and the file is sent without it.

use std::path::Path;

/// Longest side of a video thumbnail, in pixels.
pub const PREVIEW_MAX: u32 = 640;

/// Most pixels in the thumbnail uploaded beside a photo. It is saved as a
/// JPEG at quality 80 on white.
pub const THUMB_PIXELS: u64 = 250_000;

/// JPEG quality of a photo's or video's thumbnail.
const THUMB_QUALITY: f64 = 0.8;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Probe {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u64>,
    /// JPEG thumbnail.
    pub preview: Option<Vec<u8>>,
}

/// Width and height as shown: EXIF orientations 5 to 8 turn the picture
/// on its side.
fn oriented(width: u32, height: u32, orientation: i64) -> (u32, u32) {
    if (5..=8).contains(&orientation) {
        (height, width)
    } else {
        (width, height)
    }
}

/// Longer side of the thumbnail of a `width`×`height` photo: the photo's
/// own when it has no more than [`THUMB_PIXELS`], else scaled down to that
/// many pixels, keeping its shape.
fn thumb_side(width: u32, height: u32) -> u32 {
    let longer = width.max(height);
    let area = u64::from(width) * u64::from(height);
    if area <= THUMB_PIXELS {
        return longer;
    }
    let scale = (THUMB_PIXELS as f64 / area as f64).sqrt();
    ((f64::from(longer) * scale) as u32).max(1)
}

/// A clip's length in whole milliseconds, if it has one.
fn millis(seconds: f64) -> Option<u64> {
    (seconds.is_finite() && seconds > 0.0).then(|| (seconds * 1000.0).round() as u64)
}

/// A photo's size, and the JPEG thumbnail uploaded with it (see
/// [`THUMB_PIXELS`]).
pub async fn image(path: &Path) -> Probe {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || mac::image(&path))
        .await
        .unwrap_or_default()
}

/// A video's length and first-frame thumbnail, at most [`PREVIEW_MAX`]
/// pixels on its longer side.
pub async fn video(path: &Path) -> Probe {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || mac::video(&path))
        .await
        .unwrap_or_default()
}

#[cfg(not(target_os = "macos"))]
mod mac {
    use super::Probe;
    use std::path::Path;

    pub fn image(_: &Path) -> Probe {
        Probe::default()
    }
    pub fn video(_: &Path) -> Probe {
        Probe::default()
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{PREVIEW_MAX, Probe, millis, oriented};
    use std::path::Path;

    use objc2_av_foundation::{AVAssetImageGenerator, AVURLAsset};
    use super::{THUMB_QUALITY, thumb_side};
    use objc2_core_foundation::{
        CFBoolean, CFDictionary, CFMutableData, CFNumber, CFString, CFType, CFURL, CGPoint,
        CGRect, CGSize,
    };
    use objc2_core_graphics::{
        CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImage,
        CGImageAlphaInfo,
    };
    use objc2_core_media::CMTime;
    use objc2_foundation::{NSString, NSURL};
    use objc2_image_io::{
        CGImageDestination, CGImageSource, kCGImageDestinationLossyCompressionQuality,
        kCGImagePropertyOrientation, kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
        kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
        kCGImageSourceThumbnailMaxPixelSize,
    };

    fn number(props: &CFDictionary<CFString, CFType>, key: &CFString) -> Option<i64> {
        props.get(key)?.downcast::<CFNumber>().ok()?.as_i64()
    }

    pub fn image(path: &Path) -> Probe {
        let Some(url) = CFURL::from_file_path(path) else {
            return Probe::default();
        };
        // SAFETY: a file URL and no options; the properties are a
        // CFString-keyed dictionary.
        let Some(src) = (unsafe { CGImageSource::with_url(&url, None) }) else {
            return Probe::default();
        };
        let Some(props) = (unsafe { src.properties_at_index(0, None) }) else {
            return Probe::default();
        };
        let props: &CFDictionary<CFString, CFType> = unsafe { props.cast_unchecked() };
        // SAFETY: reading ImageIO's key constants.
        let (w, h, orientation) = unsafe {
            (
                number(props, kCGImagePropertyPixelWidth),
                number(props, kCGImagePropertyPixelHeight),
                number(props, kCGImagePropertyOrientation),
            )
        };
        let (Some(w), Some(h)) = (w, h) else {
            return Probe::default();
        };
        let (Ok(w), Ok(h)) = (u32::try_from(w), u32::try_from(h)) else {
            return Probe::default();
        };
        let (width, height) = oriented(w, h, orientation.unwrap_or(1));
        Probe {
            width: Some(width),
            height: Some(height),
            preview: thumbnail(&src, thumb_side(width, height)),
            ..Probe::default()
        }
    }

    /// The first picture of `src` turned upright, at most `side` pixels on
    /// its longer side, on white, as a JPEG.
    fn thumbnail(src: &CGImageSource, side: u32) -> Option<Vec<u8>> {
        let side = CFNumber::new_i64(i64::from(side));
        let yes: &CFType = CFBoolean::new(true);
        // SAFETY: reading ImageIO's key constants; the options map
        // CFString keys to the CFBoolean and CFNumber values ImageIO reads.
        let image = unsafe {
            let options = CFDictionary::<CFString, CFType>::from_slices(
                &[
                    kCGImageSourceCreateThumbnailFromImageAlways,
                    kCGImageSourceCreateThumbnailWithTransform,
                    kCGImageSourceThumbnailMaxPixelSize,
                ],
                &[yes, yes, &side],
            );
            src.thumbnail_at_index(0, Some(options.as_opaque()))?
        };
        jpeg(&*on_white(&image)?, THUMB_QUALITY)
    }

    /// `image` drawn on white, so a transparent PNG's thumbnail isn't black.
    fn on_white(image: &CGImage) -> Option<objc2_core_foundation::CFRetained<CGImage>> {
        let (w, h) = (CGImage::width(Some(image)), CGImage::height(Some(image)));
        let space = CGColorSpace::new_device_rgb()?;
        // SAFETY: a null buffer lets Core Graphics allocate one; 8-bit RGB
        // with the alpha byte skipped is a supported bitmap format.
        let ctx = unsafe {
            CGBitmapContextCreate(
                std::ptr::null_mut(),
                w,
                h,
                8,
                0,
                Some(&space),
                CGImageAlphaInfo::NoneSkipLast.0,
            )?
        };
        let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w as f64, h as f64));
        CGContext::set_rgb_fill_color(Some(&ctx), 1.0, 1.0, 1.0, 1.0);
        CGContext::fill_rect(Some(&ctx), rect);
        CGContext::draw_image(Some(&ctx), rect, Some(image));
        CGBitmapContextCreateImage(Some(&ctx))
    }

    /// `image` as a JPEG at `quality`.
    fn jpeg(image: &CGImage, quality: f64) -> Option<Vec<u8>> {
        let data = CFMutableData::new(None, 0)?;
        let quality_key: &CFString = unsafe { kCGImageDestinationLossyCompressionQuality };
        let quality = CFNumber::new_f64(quality);
        let options = CFDictionary::<CFString, CFNumber>::from_slices(&[quality_key], &[&quality]);
        // SAFETY: "public.jpeg" is a valid type identifier and the options
        // map CFString keys to CFNumbers, as ImageIO expects.
        unsafe {
            let dest = CGImageDestination::with_data(
                &data,
                &CFString::from_static_str("public.jpeg"),
                1,
                None,
            )?;
            dest.add_image(image, Some(options.as_opaque()));
            if !dest.finalize() {
                return None;
            }
        }
        Some(data.to_vec())
    }

    pub fn video(path: &Path) -> Probe {
        let Some(path) = path.to_str() else {
            return Probe::default();
        };
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        // SAFETY: a file URL and no options. The calls below block, which is
        // why this runs on a blocking thread.
        unsafe {
            let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
            let duration_ms = millis(asset.duration().seconds());
            let generator = AVAssetImageGenerator::assetImageGeneratorWithAsset(&asset);
            generator.setAppliesPreferredTrackTransform(true);
            let max = PREVIEW_MAX as f64;
            generator.setMaximumSize(CGSize::new(max, max));
            // The synchronous call is deprecated for the async one, but
            // this already runs on its own blocking thread.
            #[allow(deprecated)]
            let frame = generator
                .copyCGImageAtTime_actualTime_error(CMTime::new(0, 600), std::ptr::null_mut())
                .ok();
            Probe {
                duration_ms,
                preview: frame.and_then(|f| jpeg(&f, THUMB_QUALITY)),
                ..Probe::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sideways_orientations_swap_width_and_height() {
        assert_eq!(oriented(4032, 3024, 1), (4032, 3024));
        assert_eq!(oriented(4032, 3024, 3), (4032, 3024));
        for o in 5..=8 {
            assert_eq!(oriented(4032, 3024, o), (3024, 4032));
        }
    }

    #[test]
    fn durations_round_to_milliseconds() {
        assert_eq!(millis(4.2), Some(4200));
        assert_eq!(millis(0.0005), Some(1));
        assert_eq!(millis(0.0), None);
        assert_eq!(millis(f64::NAN), None);
        assert_eq!(millis(-1.0), None);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn reads_a_png_header() {
        // 3x2 PNG.
        let png = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            "iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAIAAAASFvFNAAAAEElEQVR4nGP4z8AAQQxwFgBB0gX7h/C5SAAAAABJRU5ErkJggg==",
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        std::fs::write(&path, png).unwrap();
        let p = image(&path).await;
        assert_eq!((p.width, p.height), (Some(3), Some(2)));
        // A small photo's thumbnail keeps its size.
        let jpeg = p.preview.unwrap();
        assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
        let thumb = dir.path().join("t.jpg");
        std::fs::write(&thumb, &jpeg).unwrap();
        let t = image(&thumb).await;
        assert_eq!((t.width, t.height), (Some(3), Some(2)));
    }

    #[test]
    fn photo_thumbnails_have_at_most_250000_pixels() {
        assert_eq!(thumb_side(400, 300), 400);
        assert_eq!(thumb_side(500, 500), 500);
        // 4032x3024 scales to about 577x433.
        let side = thumb_side(4032, 3024);
        assert_eq!(side, 577);
        let short = u64::from(side) * 3024 / 4032;
        assert!(u64::from(side) * short <= THUMB_PIXELS);
        assert_eq!(thumb_side(3024, 4032), 577);
        assert_eq!(thumb_side(2000, 1000), 707);
    }

    #[tokio::test]
    async fn unreadable_files_give_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.mp4");
        std::fs::write(&path, b"not a video").unwrap();
        assert_eq!(video(&path).await.preview, None);
        assert_eq!(image(&path).await, Probe::default());
    }

    /// A clip that ships with macOS, so the test needs no fixture.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn reads_a_system_video() {
        let path = Path::new(
            "/System/Library/CoreServices/ControlCenter.app/Contents/Resources/BentoGalleryIntroduction.mov",
        );
        if !path.exists() {
            return;
        }
        let p = video(path).await;
        assert!(p.duration_ms.unwrap() > 0);
        let jpeg = p.preview.unwrap();
        assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
        // The thumbnail is no bigger than the preview LINE shows.
        let dir = tempfile::tempdir().unwrap();
        let thumb = dir.path().join("t.jpg");
        std::fs::write(&thumb, &jpeg).unwrap();
        let t = image(&thumb).await;
        let (w, h) = (t.width.unwrap(), t.height.unwrap());
        assert!(w.max(h) <= PREVIEW_MAX && w > 0 && h > 0, "{w}x{h}");
    }
}
