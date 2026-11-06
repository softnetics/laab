//! What LINE's own app works out about a photo or video before sending it:
//! the picture's size, and a video's length and first-frame thumbnail.
//! Read on macOS with ImageIO and AVFoundation, without ffmpeg. Anything
//! that can't be read is left out and the file is sent without it.

use std::path::Path;

/// Longest side of a video thumbnail, in pixels.
pub const PREVIEW_MAX: u32 = 640;

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

/// A clip's length in whole milliseconds, if it has one.
fn millis(seconds: f64) -> Option<u64> {
    (seconds.is_finite() && seconds > 0.0).then(|| (seconds * 1000.0).round() as u64)
}

/// A photo's size, from its header.
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
    use objc2_core_foundation::{
        CFDictionary, CFMutableData, CFNumber, CFString, CFType, CFURL, CGSize,
    };
    use objc2_core_graphics::CGImage;
    use objc2_core_media::CMTime;
    use objc2_foundation::{NSString, NSURL};
    use objc2_image_io::{
        CGImageDestination, CGImageSource, kCGImageDestinationLossyCompressionQuality,
        kCGImagePropertyOrientation, kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
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
        let props = unsafe {
            CGImageSource::with_url(&url, None).and_then(|src| src.properties_at_index(0, None))
        };
        let Some(props) = props else {
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
            ..Probe::default()
        }
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
                preview: frame.and_then(|f| jpeg(&f, 0.8)),
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
        assert_eq!(p.preview, None);
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
        // The thumbnail is no bigger than the app's preview.
        let dir = tempfile::tempdir().unwrap();
        let thumb = dir.path().join("t.jpg");
        std::fs::write(&thumb, &jpeg).unwrap();
        let t = image(&thumb).await;
        let (w, h) = (t.width.unwrap(), t.height.unwrap());
        assert!(w.max(h) <= PREVIEW_MAX && w > 0 && h > 0, "{w}x{h}");
    }
}
