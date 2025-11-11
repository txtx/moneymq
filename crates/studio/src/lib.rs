use std::borrow::Cow;

use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode},
};
use include_dir::{Dir, include_dir};
use mime_guess::from_path;

pub const OUT_DIR: &str = env!("OUT_DIR");
pub static ASSETS: Dir<'_> = include_dir!("$OUT_DIR/moneymq-studio-ui");

pub async fn serve_studio_static_files(req: Request) -> Response<Body> {
    // Path without leading slash
    let path = req.uri().path().trim_start_matches('/');

    let file = if path.is_empty() {
        // root → index.html
        ASSETS.get_file("index.html")
    } else {
        ASSETS
            .get_file(path)
            .or_else(|| ASSETS.get_file(format!("{path}.html")))
            .or_else(|| ASSETS.get_file(format!("{path}/index.html")))
            .or_else(|| ASSETS.get_file("index.html"))
    };

    match file {
        Some(file) => {
            let bytes: Cow<'_, [u8]> = std::borrow::Cow::Borrowed(file.contents());
            let mime = from_path(file.path()).first_or_octet_stream();

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime.as_ref())
                .body(Body::from(bytes.to_owned()))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    }
}
