//! A picture of one element of a page, for the agent a person pointed at it.
//!
//! The preview's page lives on an origin the window may not read, so the
//! picture is not taken from the window. The page is opened again in a
//! headless Chrome or Chromium at the same width, tall enough to reach the
//! element, and the element's place is cut out of that. It is the page freshly
//! rendered, not the moment the person saw: a menu held open or text typed in
//! is not in it, and the note that goes with it says so.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

/// Room left around the element, so its edges and what touches them show.
const PAD: u32 = 12;
/// Past this a page is not measured to the element: a picture of something
/// twenty screens down is not worth the render.
const TALLEST: u32 = 12_000;
const TOO_LONG: Duration = Duration::from_secs(25);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct Scroll {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

/// Where the element was: its box as the page's viewport saw it, how far the
/// page had been scrolled, and how big the viewport was.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Placed {
    #[serde(rename = "box")]
    pub rect: Rect,
    #[serde(default)]
    pub scroll: Scroll,
    pub viewport: Size,
}

impl Placed {
    /// The element's box on the page itself rather than on the screen.
    fn on_the_page(&self) -> Rect {
        Rect {
            x: self.rect.x + self.scroll.x,
            y: self.rect.y + self.scroll.y,
            ..self.rect
        }
    }
}

/// How big the headless window has to be: as wide as the person's view, so
/// the page lays out the same, and as tall as it takes to reach the element.
pub fn window_for(placed: &Placed) -> (u32, u32) {
    let element = placed.on_the_page();
    let width = placed.viewport.width.round().clamp(200.0, 4000.0) as u32;
    let reach = (element.y + element.height).ceil().max(0.0) as u32 + PAD;
    let height = (placed.viewport.height.round().max(200.0) as u32).max(reach).min(TALLEST);
    (width, height)
}

/// The part of a picture `width` by `height` to keep: the element and a little
/// room around it, inside the picture. None when the element is not in it.
pub fn crop_of(placed: &Placed, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let element = placed.on_the_page();
    let left = (element.x.floor() - f64::from(PAD)).max(0.0) as u32;
    let top = (element.y.floor() - f64::from(PAD)).max(0.0) as u32;
    let right = ((element.x + element.width).ceil() + f64::from(PAD)).min(f64::from(width)) as u32;
    let bottom = ((element.y + element.height).ceil() + f64::from(PAD)).min(f64::from(height)) as u32;

    (right > left && bottom > top && left < width && top < height).then(|| (left, top, right - left, bottom - top))
}

/// A rectangle cut out of a PNG, as a PNG.
pub fn crop_png(picture: &[u8], (left, top, width, height): (u32, u32, u32, u32)) -> Result<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(picture));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().context("the picture is not a PNG")?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;

    if left + width > info.width || top + height > info.height {
        bail!("the part to keep lies outside the picture");
    }

    let per_pixel = info.color_type.samples();
    let mut kept = Vec::with_capacity((width * height) as usize * per_pixel);
    for row in top..top + height {
        let start = row as usize * info.line_size + left as usize * per_pixel;
        kept.extend_from_slice(&pixels[start..start + width as usize * per_pixel]);
    }

    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(info.color_type);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&kept)?;
    }
    Ok(out)
}

/// A Chrome or Chromium to render with: the one named in `AGENTLAND_CHROME`,
/// else the first on the path, else where the installers put it.
pub fn chrome() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("AGENTLAND_CHROME").map(PathBuf::from) {
        return named.exists().then_some(named);
    }

    let names = [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "microsoft-edge",
        "chrome",
    ];
    let on_path = std::env::var_os("PATH").into_iter().flat_map(|path| {
        std::env::split_paths(&path)
            .flat_map(|dir| {
                names.iter().flat_map(move |name| {
                    [dir.join(name), dir.join(format!("{name}.exe"))]
                })
            })
            .collect::<Vec<_>>()
    });

    let installed = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ]
    .into_iter()
    .map(PathBuf::from);

    on_path.chain(installed).find(|candidate| candidate.is_file())
}

fn stamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default()
}

/// Render `url` headless and keep the element's part of it, written into
/// `folder`. Returns the picture's path.
pub async fn photograph(url: &str, placed: &Placed, folder: &Path) -> Result<PathBuf> {
    let browser = chrome().ok_or_else(|| anyhow!("no Chrome or Chromium on this machine to take the picture with"))?;
    std::fs::create_dir_all(folder)?;

    let taken = stamp();
    let profile = folder.join(format!(".profile-{taken}"));
    let whole = folder.join(format!(".{taken}-page.png"));
    let (width, height) = window_for(placed);

    let rendered = tokio::time::timeout(
        TOO_LONG,
        tokio::process::Command::new(&browser)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--hide-scrollbars")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--virtual-time-budget=2000")
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--window-size={width},{height}"))
            .arg(format!("--screenshot={}", whole.display()))
            .arg(url)
            .kill_on_drop(true)
            .output(),
    )
    .await;
    let _ = std::fs::remove_dir_all(&profile);

    match rendered {
        Err(_) => bail!("the page took longer than {} seconds to render", TOO_LONG.as_secs()),
        Ok(Err(error)) => return Err(error).context("Chrome could not be started"),
        Ok(Ok(_)) => {}
    }

    let picture = std::fs::read(&whole).context("Chrome rendered no picture of the page")?;
    let _ = std::fs::remove_file(&whole);

    let (left, top, kept_width, kept_height) =
        crop_of(placed, width, height).ok_or_else(|| anyhow!("the element is not on the rendered page"))?;
    let cropped = crop_png(&picture, (left, top, kept_width, kept_height))?;

    let path = folder.join(format!("{taken}-element.png"));
    std::fs::write(&path, cropped)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed(x: f64, y: f64, scroll_y: f64) -> Placed {
        Placed {
            rect: Rect { x, y, width: 120.0, height: 40.0 },
            scroll: Scroll { x: 0.0, y: scroll_y },
            viewport: Size { width: 1280.0, height: 800.0 },
        }
    }

    fn picture(width: u32, height: u32) -> Vec<u8> {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&[x as u8, y as u8, 0x55, 0xff]);
            }
        }
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        out
    }

    #[test]
    fn the_window_is_as_wide_as_the_view_and_reaches_the_element() {
        assert_eq!(window_for(&placed(40.0, 300.0, 0.0)), (1280, 800));
        assert_eq!(window_for(&placed(40.0, 300.0, 1500.0)), (1280, 1852), "scrolled down, it grows to reach it");
    }

    #[test]
    fn a_page_is_not_rendered_to_the_end_of_the_world() {
        assert_eq!(window_for(&placed(40.0, 300.0, 90_000.0)).1, TALLEST);
    }

    #[test]
    fn the_crop_is_the_element_and_a_little_room_on_the_page() {
        assert_eq!(crop_of(&placed(40.0, 300.0, 1500.0), 1280, 1852), Some((28, 1788, 144, 64)));
    }

    #[test]
    fn the_crop_stays_inside_the_picture() {
        assert_eq!(crop_of(&placed(0.0, 0.0, 0.0), 1280, 800), Some((0, 0, 132, 52)));
        assert_eq!(crop_of(&placed(40.0, 5000.0, 0.0), 1280, 800), None, "below what was rendered");
    }

    #[test]
    fn a_crop_keeps_exactly_the_pixels_asked_for() {
        let cut = crop_png(&picture(64, 48), (10, 20, 5, 3)).unwrap();

        let mut reader = png::Decoder::new(std::io::Cursor::new(cut)).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (5, 3));
        assert_eq!(&pixels[..4], &[10, 20, 0x55, 0xff], "the top left is the pixel at (10, 20)");
        assert!(crop_png(&picture(64, 48), (60, 0, 10, 10)).is_err());
    }

    /// Needs a Chrome or Chromium; run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn a_real_page_is_photographed_down_to_the_element() {
        if chrome().is_none() {
            return;
        }
        let folder = std::env::temp_dir().join(format!("agentland-shots-{}", std::process::id()));
        let page = folder.join("page.html");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            &page,
            r#"<html><body style="margin:0;height:3000px;background:#fff"><div style="position:absolute;left:40px;top:1800px;width:120px;height:40px;background:#1f6feb"></div></body></html>"#,
        )
        .unwrap();

        let shot = photograph(&format!("file://{}", page.display()), &placed(40.0, 300.0, 1500.0), &folder)
            .await
            .unwrap();

        let mut reader = png::Decoder::new(std::fs::File::open(&shot).unwrap()).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (144, 64));
        let middle = ((32 * info.width + 72) as usize) * info.color_type.samples();
        assert_eq!(&pixels[middle..middle + 3], &[0x1f, 0x6f, 0xeb], "the middle of the picture is the element");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
