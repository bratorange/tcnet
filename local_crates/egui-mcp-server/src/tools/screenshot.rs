//! Screenshot tool implementations

use super::{ToolResult, error_response, not_connected_error, parse_element_id};
use crate::ipc_client::IpcClient;
use rmcp::model::Content;
use serde_json::json;

#[cfg(target_os = "linux")]
use crate::atspi_client::AtspiClient;

/// Take a screenshot of the application
pub async fn take_screenshot(
    ipc_client: &IpcClient,
    save_to_file: bool,
) -> Result<Content, String> {
    if !ipc_client.is_socket_available() {
        return Err(not_connected_error());
    }

    match ipc_client.take_screenshot().await {
        Ok((data, _format)) => {
            if save_to_file {
                Ok(save_screenshot_to_file(&data))
            } else {
                Ok(Content::image(&data, "image/png"))
            }
        }
        Err(e) => Err(error_response(
            "screenshot_error",
            format!("Failed to take screenshot: {}", e),
        )),
    }
}

/// Take a screenshot of a specific element
pub async fn screenshot_element(
    app_name: &str,
    ipc_client: &IpcClient,
    id_str: &str,
    save_to_file: bool,
) -> Result<Content, String> {
    let id = parse_element_id(id_str)?;

    if !ipc_client.is_socket_available() {
        return Err(not_connected_error());
    }

    #[cfg(target_os = "linux")]
    {
        let client = match AtspiClient::new().await {
            Ok(c) => c,
            Err(e) => return Err(super::atspi_connection_error(e)),
        };

        // Get element bounds
        let bounds = match client.get_bounds(app_name, id).await {
            Ok(Some(b)) => b,
            Ok(None) => {
                return Err(error_response(
                    "no_bounds",
                    format!("Element {} has no bounds", id),
                ));
            }
            Err(e) => {
                return Err(error_response(
                    "atspi_error",
                    format!("Failed to get element bounds: {}", e),
                ));
            }
        };

        // Take full screenshot and crop
        match ipc_client.take_screenshot().await {
            Ok((data, _format)) => {
                match crop_screenshot(&data, bounds.x, bounds.y, bounds.width, bounds.height) {
                    Ok(cropped) => {
                        if save_to_file {
                            Ok(save_screenshot_to_file(&cropped))
                        } else {
                            Ok(Content::image(&cropped, "image/png"))
                        }
                    }
                    Err(e) => Err(error_response("crop_error", e)),
                }
            }
            Err(e) => Err(error_response(
                "screenshot_error",
                format!("Failed to take screenshot: {}", e),
            )),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app_name, id, save_to_file);
        Err(error_response(
            "not_available",
            "screenshot_element requires AT-SPI on Linux.",
        ))
    }
}

/// Take a screenshot of a specific region
pub async fn screenshot_region(
    ipc_client: &IpcClient,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    save_to_file: bool,
) -> Result<Content, String> {
    if !ipc_client.is_socket_available() {
        return Err(not_connected_error());
    }

    match ipc_client.take_screenshot().await {
        Ok((data, _format)) => match crop_screenshot(&data, x, y, width, height) {
            Ok(cropped) => {
                if save_to_file {
                    Ok(save_screenshot_to_file(&cropped))
                } else {
                    Ok(Content::image(&cropped, "image/png"))
                }
            }
            Err(e) => Err(error_response("crop_error", e)),
        },
        Err(e) => Err(error_response(
            "screenshot_error",
            format!("Failed to take screenshot: {}", e),
        )),
    }
}

/// Compare two screenshots and return similarity score
pub fn compare_screenshots(
    base64_a: Option<&str>,
    base64_b: Option<&str>,
    path_a: Option<&str>,
    path_b: Option<&str>,
    algorithm: Option<&str>,
) -> ToolResult {
    use image::DynamicImage;
    use image_compare::Algorithm;

    // Load first image
    let img_a: DynamicImage = match load_image(base64_a, path_a) {
        Ok(img) => img,
        Err(e) => return e,
    };

    // Load second image
    let img_b: DynamicImage = match load_image(base64_b, path_b) {
        Ok(img) => img,
        Err(e) => return e,
    };

    // Convert to grayscale for comparison
    let gray_a = img_a.to_luma8();
    let gray_b = img_b.to_luma8();

    // Check dimensions match
    if gray_a.dimensions() != gray_b.dimensions() {
        return json!({
            "score": 0.0,
            "identical": false,
            "error": "dimension_mismatch",
            "message": format!(
                "Images have different dimensions: {:?} vs {:?}",
                gray_a.dimensions(),
                gray_b.dimensions()
            )
        })
        .to_string();
    }

    // Compare based on algorithm
    let score = match algorithm.unwrap_or("hybrid") {
        "mssim" => {
            match image_compare::gray_similarity_structure(
                &Algorithm::MSSIMSimple,
                &gray_a,
                &gray_b,
            ) {
                Ok(result) => result.score,
                Err(e) => {
                    return error_response("compare_error", format!("Comparison failed: {}", e));
                }
            }
        }
        "rms" => {
            match image_compare::gray_similarity_structure(
                &Algorithm::RootMeanSquared,
                &gray_a,
                &gray_b,
            ) {
                Ok(result) => result.score,
                Err(e) => {
                    return error_response("compare_error", format!("Comparison failed: {}", e));
                }
            }
        }
        _ => {
            // Hybrid: combine MSSIM and RMS
            let mssim = match image_compare::gray_similarity_structure(
                &Algorithm::MSSIMSimple,
                &gray_a,
                &gray_b,
            ) {
                Ok(result) => result.score,
                Err(e) => return error_response("compare_error", format!("MSSIM failed: {}", e)),
            };
            let rms = match image_compare::gray_similarity_structure(
                &Algorithm::RootMeanSquared,
                &gray_a,
                &gray_b,
            ) {
                Ok(result) => result.score,
                Err(e) => return error_response("compare_error", format!("RMS failed: {}", e)),
            };
            (mssim + rms) / 2.0
        }
    };

    json!({
        "score": score,
        "identical": score > 0.9999,
        "algorithm": algorithm.unwrap_or("hybrid")
    })
    .to_string()
}

/// Generate a visual diff image
pub fn diff_screenshots(
    base64_a: Option<&str>,
    base64_b: Option<&str>,
    path_a: Option<&str>,
    path_b: Option<&str>,
    save_to_file: bool,
) -> Result<Content, String> {
    use base64::Engine;

    // Load first image
    let img_a = load_image(base64_a, path_a)?;

    // Load second image
    let img_b = load_image(base64_b, path_b)?;

    // Generate diff image
    let rgba_a = img_a.to_rgba8();
    let rgba_b = img_b.to_rgba8();

    let (width_a, height_a) = rgba_a.dimensions();
    let (width_b, height_b) = rgba_b.dimensions();
    let max_width = width_a.max(width_b);
    let max_height = height_a.max(height_b);

    let mut diff_img = image::RgbaImage::new(max_width, max_height);

    for y in 0..max_height {
        for x in 0..max_width {
            let pixel_a = if x < width_a && y < height_a {
                *rgba_a.get_pixel(x, y)
            } else {
                image::Rgba([0, 0, 0, 255])
            };
            let pixel_b = if x < width_b && y < height_b {
                *rgba_b.get_pixel(x, y)
            } else {
                image::Rgba([0, 0, 0, 255])
            };

            let diff = if pixel_a == pixel_b {
                // Same pixel - show grayscale version
                let gray = ((pixel_a[0] as u32 + pixel_a[1] as u32 + pixel_a[2] as u32) / 3) as u8;
                image::Rgba([gray, gray, gray, 128])
            } else {
                // Different pixel - highlight in red
                image::Rgba([255, 0, 0, 255])
            };
            diff_img.put_pixel(x, y, diff);
        }
    }

    // Encode to PNG
    let mut buffer = std::io::Cursor::new(Vec::new());
    diff_img
        .write_to(&mut buffer, image::ImageFormat::Png)
        .map_err(|e| {
            error_response(
                "encode_error",
                format!("Failed to encode diff image: {}", e),
            )
        })?;

    let png_data = buffer.into_inner();
    let base64_data = base64::engine::general_purpose::STANDARD.encode(&png_data);

    if save_to_file {
        Ok(save_screenshot_to_file(&base64_data))
    } else {
        Ok(Content::image(&base64_data, "image/png"))
    }
}

/// Load an image from base64 or file path
fn load_image(
    base64_data: Option<&str>,
    path: Option<&str>,
) -> Result<image::DynamicImage, ToolResult> {
    use base64::Engine;

    if let Some(b64) = base64_data {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| {
                error_response("decode_error", format!("Failed to decode base64: {}", e))
            })?;
        image::load_from_memory(&bytes)
            .map_err(|e| error_response("image_error", format!("Failed to load image: {}", e)))
    } else if let Some(p) = path {
        image::open(p)
            .map_err(|e| error_response("file_error", format!("Failed to open file: {}", e)))
    } else {
        Err(error_response(
            "missing_input",
            "Either base64 data or file path must be provided",
        ))
    }
}

/// Crop a screenshot to specific bounds
fn crop_screenshot(
    base64_data: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Result<String, String> {
    use base64::Engine;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    let img =
        image::load_from_memory(&bytes).map_err(|e| format!("Failed to load image: {}", e))?;

    let cropped = img.crop_imm(x as u32, y as u32, width as u32, height as u32);

    let mut buffer = std::io::Cursor::new(Vec::new());
    cropped
        .write_to(&mut buffer, image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode cropped image: {}", e))?;

    Ok(base64::engine::general_purpose::STANDARD.encode(buffer.into_inner()))
}

/// Save base64-encoded PNG data to a temp file
pub fn save_screenshot_to_file(data: &str) -> Content {
    use base64::Engine;

    match base64::engine::general_purpose::STANDARD.decode(data) {
        Ok(png_bytes) => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let file_path = format!("/tmp/egui-mcp-screenshot-{}.png", timestamp);

            match std::fs::write(&file_path, png_bytes.as_slice()) {
                Ok(()) => Content::text(
                    json!({
                        "file_path": file_path,
                        "size_bytes": png_bytes.len()
                    })
                    .to_string(),
                ),
                Err(e) => Content::text(
                    json!({
                        "error": "file_write_error",
                        "message": format!("Failed to write screenshot file: {}", e)
                    })
                    .to_string(),
                ),
            }
        }
        Err(e) => Content::text(
            json!({
                "error": "decode_error",
                "message": format!("Failed to decode base64 data: {}", e)
            })
            .to_string(),
        ),
    }
}
