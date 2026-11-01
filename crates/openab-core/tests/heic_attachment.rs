//! Needs ImageMagick with HEIC support on PATH. The broker's runtime image has it and a plain
//! Rust toolchain does not, so run with `cargo test --test heic_attachment -- --ignored`.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use openab_core::acp::ContentBlock;
use openab_core::media::{download_and_encode_image, MediaFetchError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// 64x48 stored pixels, red and blue over green and white, with the `irot` of an iPhone portrait
// shot: only a rotation applied exactly once, with no flip, gives the layout `assert_upright` wants.
const HEIC: &[u8] = include_bytes!("fixtures/rotated_quadrants.heic");

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const WHITE: [u8; 3] = [255, 255, 255];

async fn serve_once(body: &'static [u8]) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 2048];
        let _ = socket.read(&mut request).await.unwrap();
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: image/heic\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.write_all(body).await.unwrap();
    });
    format!("http://{addr}/photo")
}

async fn download(
    body: &'static [u8],
    mime_hint: Option<&str>,
    filename: &str,
) -> Result<ContentBlock, MediaFetchError> {
    let url = serve_once(body).await;
    download_and_encode_image(&url, mime_hint, filename, body.len() as u64, None).await
}

async fn inline_image(mime_hint: Option<&str>, filename: &str) -> image::RgbImage {
    let block = download(HEIC, mime_hint, filename)
        .await
        .unwrap_or_else(|e| panic!("attachment rejected: {e}"));
    let ContentBlock::Image { media_type, data } = block else {
        panic!("expected an inline image block");
    };
    assert_eq!(media_type, "image/jpeg");
    image::load_from_memory(&BASE64.decode(data).unwrap())
        .unwrap()
        .to_rgb8()
}

fn assert_upright(img: &image::RgbImage) {
    assert_eq!(img.dimensions(), (48, 64));
    for (x, y, expected) in [
        (12, 16, GREEN),
        (36, 16, RED),
        (12, 48, WHITE),
        (36, 48, BLUE),
    ] {
        let actual = img.get_pixel(x, y).0;
        let close = actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) < 70);
        assert!(close, "({x},{y}) is {actual:?}, expected {expected:?}");
    }
}

#[tokio::test]
#[ignore = "needs ImageMagick with HEIC support"]
async fn iphone_photo_reaches_the_agent_as_an_upright_jpeg() {
    assert_upright(&inline_image(Some("image/heic"), "photo.heic").await);
}

#[tokio::test]
#[ignore = "needs ImageMagick with HEIC support"]
async fn filename_alone_identifies_a_heic_attachment() {
    assert_upright(&inline_image(None, "PHOTO.HEIC").await);
}

#[tokio::test]
#[ignore = "needs ImageMagick with HEIC support"]
async fn truncated_heic_is_refused_rather_than_half_decoded() {
    let result = download(&HEIC[..HEIC.len() / 2], Some("image/heic"), "photo.heic").await;
    assert!(matches!(
        result,
        Err(MediaFetchError::InvalidImageBody { .. })
    ));
}
