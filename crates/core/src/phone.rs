/// Getting a phone in without typing anything.
///
/// The address a phone needs carries a token, which nobody wants to type off a
/// screen. A code it can point a camera at is the whole answer.
use qrcode::render::svg;
use qrcode::QrCode;

/// The address to hand a phone. The trailing slash matters: the page is served
/// from a folder, and without it a browser asks for a file.
pub fn url_for(host: &str, port: u16, token: &str) -> String {
    format!("http://{host}:{port}/mobile/?token={token}")
}

/// Whether a phone could reach this at all.
///
/// A core bound to the loopback address answers only the machine it runs on, so
/// a code for it would be a code that goes nowhere.
pub fn reachable(host: &str) -> bool {
    !matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// The address as something a camera can read.
pub fn as_a_code(url: &str) -> Option<String> {
    let code = QrCode::new(url.as_bytes()).ok()?;

    Some(
        code.render()
            .min_dimensions(220, 220)
            .dark_color(svg::Color("#04100f"))
            .light_color(svg::Color("#ffffff"))
            .build(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_carries_the_token_and_ends_in_a_slash() {
        let url = url_for("192.168.1.128", 9470, "abc123");

        assert_eq!(url, "http://192.168.1.128:9470/mobile/?token=abc123");
    }

    #[test]
    fn a_core_that_only_answers_itself_is_no_use_to_a_phone() {
        assert!(!reachable("127.0.0.1"));
        assert!(!reachable("localhost"));
        assert!(reachable("0.0.0.0"));
        assert!(reachable("192.168.1.128"));
    }

    #[test]
    fn the_code_is_an_image_a_camera_can_read() {
        let svg = as_a_code("http://192.168.1.128:9470/mobile/?token=abc").expect("a code");

        assert!(svg.contains("<svg"), "the renderer wraps it: {}", &svg[..60.min(svg.len())]);
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn a_url_too_long_to_encode_says_so_rather_than_drawing_nonsense() {
        assert_eq!(as_a_code(&"x".repeat(8_000)), None);
    }
}
