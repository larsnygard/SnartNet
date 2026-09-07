//! Avatar decoding and portable invitation QR rendering/export.
use super::*;

pub(crate) fn app_output_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let downloads = PathBuf::from(home).join("Downloads");
        if downloads.is_dir() {
            return downloads;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
}

pub(crate) fn qr_export_path(ext: &str) -> PathBuf {
    app_output_dir().join(format!("snartnet-invite-{}.{}", unix_secs(), ext))
}

pub(crate) fn qr_code_from_data(data: &str) -> Result<qrcode::QrCode, String> {
    qrcode::QrCode::with_error_correction_level(data.as_bytes(), qrcode::EcLevel::L)
        .map_err(|e| format!("QR generation failed: {e}"))
}

pub(crate) fn render_qr_svg_string(data: &str) -> Result<String, String> {
    use qrcode::render::svg as qrcode_svg;

    let code = qr_code_from_data(data)?;
    Ok(code
        .render::<qrcode_svg::Color>()
        .quiet_zone(true)
        .min_dimensions(280, 280)
        .dark_color(qrcode_svg::Color("#000000"))
        .light_color(qrcode_svg::Color("#ffffff"))
        .build())
}

pub(crate) fn save_qr_svg(data: &str) -> Result<PathBuf, String> {
    let path = qr_export_path("svg");
    let svg = render_qr_svg_string(data)?;
    std::fs::write(&path, svg).map_err(|e| format!("write failed: {e}"))?;
    Ok(path)
}

pub(crate) fn save_qr_png(data: &str) -> Result<PathBuf, String> {
    let path = qr_export_path("png");
    let code = qr_code_from_data(data)?;
    let image = code
        .render::<::image::Luma<u8>>()
        .quiet_zone(true)
        .min_dimensions(640, 640)
        .build();
    image
        .save(&path)
        .map_err(|e| format!("png save failed: {e}"))?;
    Ok(path)
}

pub(crate) fn save_qr_jpg(data: &str) -> Result<PathBuf, String> {
    let path = qr_export_path("jpg");
    let code = qr_code_from_data(data)?;
    let image = code
        .render::<::image::Luma<u8>>()
        .quiet_zone(true)
        .min_dimensions(640, 640)
        .build();
    let dynimg = ::image::DynamicImage::ImageLuma8(image);
    dynimg
        .save_with_format(&path, ::image::ImageFormat::Jpeg)
        .map_err(|e| format!("jpg save failed: {e}"))?;
    Ok(path)
}

pub(crate) fn image_handle_from_data_url(data_url: &str) -> Option<iced::widget::image::Handle> {
    let (_, b64) = data_url.split_once("base64,")?;
    let bytes = general_purpose::STANDARD.decode(b64).ok()?;
    Some(iced::widget::image::Handle::from_bytes(bytes))
}

pub(crate) fn load_avatar_data_url_from_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("empty path".to_string());
    }

    let img = ::image::open(path).map_err(|e| format!("open failed: {e}"))?;
    let resized = img.resize(256, 256, ::image::imageops::FilterType::Lanczos3);
    let mut png = Vec::new();
    resized
        .write_to(&mut Cursor::new(&mut png), ::image::ImageFormat::Png)
        .map_err(|e| format!("encode failed: {e}"))?;

    let b64 = general_purpose::STANDARD.encode(png);
    Ok(format!("data:image/png;base64,{b64}"))
}

/// Render `data` as an SVG QR image handle suitable for display in iced.
pub(crate) fn generate_qr_svg_handle(data: &str) -> svg::Handle {
    match render_qr_svg_string(data) {
        Ok(svg_text) => svg::Handle::from_memory(svg_text.into_bytes()),
        Err(_) => svg::Handle::from_memory(
            r#"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 280 280' width='280' height='280'><rect width='100%' height='100%' fill='white'/><text x='12' y='28' font-size='16' font-family='monospace' fill='black'>QR generation failed</text></svg>"#
                .as_bytes()
                .to_vec(),
        ),
    }
}

/// Decode saved invitations without requiring a webcam or an external QR service.
pub(crate) fn read_qr_file(path: &str) -> Result<String, String> {
    let mut reader = ::image::ImageReader::open(path)
        .map_err(|e| format!("Cannot open QR image: {e}"))?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = ::image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| format!("Cannot read QR image: {e}"))?
        .to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(image);
    for grid in prepared.detect_grids() {
        if let Ok((_, content)) = grid.decode() {
            if ContactInvite::parse(&content).is_ok() {
                return Ok(content);
            }
        }
    }
    Err("No valid SnartNet invitation found in this image".into())
}
