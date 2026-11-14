use base64::Engine;
use serde_json::{json, Value};
use crate::core::error::ManagerError;

// 在原生边界校验实际图片字节，避免仅凭前端声明接受任意数据。
pub(super) fn validate(value: &Value) -> Result<Vec<Value>, ManagerError> {
    if value.is_null() { return Ok(Vec::new()); }
    let images = value.as_array().ok_or_else(|| ManagerError::System("图片附件格式错误".into()))?;
    if images.len() > 4 { return Err(ManagerError::System("每条消息最多 4 张图片".into())); }
    let mut total = 0;
    images.iter().enumerate().map(|(index, image)| {
        let data = image["dataUrl"].as_str().unwrap_or("");
        if data.len() > 7_000_000 { return Err(ManagerError::System("单张图片最多 5 MB".into())); }
        let (header, encoded) = data.split_once(',').ok_or_else(|| ManagerError::System("图片数据无效".into()))?;
        let mime = match header { "data:image/png;base64" => "image/png", "data:image/jpeg;base64" => "image/jpeg", "data:image/webp;base64" => "image/webp", "data:image/gif;base64" => "image/gif", _ => return Err(ManagerError::System("仅支持 PNG、JPEG、WebP 和 GIF 图片".into())) };
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_| ManagerError::System("图片编码无效".into()))?;
        let valid = match mime { "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]), "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"), _ => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") };
        total += bytes.len();
        if !valid || bytes.len() > 5 * 1024 * 1024 || total > 12 * 1024 * 1024 { return Err(ManagerError::System("图片格式或大小不符合要求（单张 5 MB，合计 12 MB）".into())); }
        Ok(json!({ "id": image["id"].as_str().unwrap_or(&index.to_string()), "name": image["name"].as_str().unwrap_or("图片").chars().take(200).collect::<String>(), "mimeType": mime, "dataUrl": data }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_image_bytes_and_count() {
        let image = json!({"dataUrl": "data:image/png;base64,iVBORw0KGgo="});
        assert_eq!(validate(&json!([image.clone()])).unwrap()[0]["mimeType"], "image/png");
        assert!(validate(&json!([{"dataUrl":"data:image/png;base64,dGV4dA=="}])).is_err());
        assert!(validate(&json!([image.clone(),image.clone(),image.clone(),image.clone(),image])).is_err());
    }
}
