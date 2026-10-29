use image::RgbaImage;

#[cfg(target_os = "linux")]
pub mod portal;

pub struct Frame {
    pub node: u32,
    pub logical_size: Option<(i32, i32)>,
    pub image: RgbaImage,
}

// 门户坐标属于合成器，像素尺寸属于视频流；预览按授权顺序横向排列，不能混用两者。
pub fn compose(frames: &[&Frame]) -> Result<RgbaImage, String> {
    let width = frames.iter().try_fold(0u32, |width, frame| {
        width.checked_add(frame.image.width()).ok_or("屏幕宽度溢出")
    })?;
    let height = frames
        .iter()
        .map(|frame| frame.image.height())
        .max()
        .unwrap_or(0);
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 40_000_000 {
        return Err("授权画面为空或超过 4000 万像素，请减少共享屏幕".into());
    }
    let mut image = RgbaImage::new(width, height);
    let mut left = 0;
    for frame in frames {
        image::imageops::replace(&mut image, &frame.image, left, 0);
        left += i64::from(frame.image.width());
    }
    Ok(image)
}

pub fn pointer_target(frames: &[&Frame], x: i32, y: i32) -> Result<(u32, f64, f64), String> {
    let mut left = 0i64;
    for frame in frames {
        let width = i64::from(frame.image.width());
        let height = i64::from(frame.image.height());
        if i64::from(x) >= left && i64::from(x) < left + width && y >= 0 && i64::from(y) < height {
            let (logical_width, logical_height) = frame
                .logical_size
                .filter(|(width, height)| *width > 0 && *height > 0)
                .unwrap_or((width as i32, height as i32));
            return Ok((
                frame.node,
                (i64::from(x) - left) as f64 * f64::from(logical_width) / width as f64,
                f64::from(y) * f64::from(logical_height) / height as f64,
            ));
        }
        left += width;
    }
    Err("滚动位置不在已授权的屏幕内，请重新框选".into())
}

pub fn decode_frame(
    bytes: &[u8],
    width: u32,
    height: u32,
    offset: usize,
    size: usize,
    stride: i32,
    bgr: bool,
    alpha: bool,
) -> Result<RgbaImage, String> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 40_000_000 {
        return Err("视频帧尺寸无效或过大".into());
    }
    let row = width as usize * 4;
    let pitch = stride.unsigned_abs() as usize;
    let end = offset
        .checked_add(size)
        .filter(|end| *end <= bytes.len())
        .ok_or("视频缓冲区范围无效")?;
    let span = pitch
        .checked_mul(height as usize - 1)
        .and_then(|span| span.checked_add(row))
        .ok_or("视频帧长度溢出")?;
    if pitch < row || span > size {
        return Err("视频帧步长或长度无效".into());
    }
    let source = &bytes[offset..end];
    let mut output = RgbaImage::new(width, height);
    for (y, target) in output.as_mut().chunks_exact_mut(row).enumerate() {
        let source_y = if stride < 0 {
            height as usize - 1 - y
        } else {
            y
        };
        target.copy_from_slice(&source[source_y * pitch..source_y * pitch + row]);
        for pixel in target.chunks_exact_mut(4) {
            if bgr {
                pixel.swap(0, 2);
            }
            if !alpha {
                pixel[3] = 255;
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_frames_respect_offset_padding_channels_and_alpha() {
        let bytes = [99, 99, 3, 2, 1, 0, 0, 0, 0, 0, 6, 5, 4, 0];
        let image = decode_frame(&bytes, 1, 2, 2, 12, 8, true, false).unwrap();
        assert_eq!(image.into_raw(), [1, 2, 3, 255, 4, 5, 6, 255]);
        assert_eq!(
            decode_frame(&bytes, 1, 2, 2, 12, -8, false, true)
                .unwrap()
                .into_raw(),
            [6, 5, 4, 0, 3, 2, 1, 0]
        );
        assert!(decode_frame(&bytes, 1, 2, 2, 11, 8, true, false).is_err());
        assert!(decode_frame(&bytes, 2, 1, 0, 14, 4, false, true).is_err());
        assert!(decode_frame(&bytes, 1, 1, usize::MAX, 1, 4, false, true).is_err());
    }

    #[test]
    fn wayland_selection_and_pointer_share_stream_pixel_coordinates() {
        let frames = vec![
            Frame {
                node: 8,
                logical_size: Some((2, 1)),
                image: RgbaImage::from_pixel(4, 2, image::Rgba([1, 2, 3, 255])),
            },
            Frame {
                node: 9,
                logical_size: Some((3, 2)),
                image: RgbaImage::from_pixel(6, 4, image::Rgba([4, 5, 6, 255])),
            },
        ];
        let frames = frames.iter().collect::<Vec<_>>();
        let image = compose(&frames).unwrap();
        assert_eq!(image.dimensions(), (10, 4));
        assert_eq!(image.get_pixel(4, 0).0, [4, 5, 6, 255]);
        assert_eq!(pointer_target(&frames, 3, 1).unwrap(), (8, 1.5, 0.5));
        assert_eq!(pointer_target(&frames, 6, 2).unwrap(), (9, 1.0, 1.0));
        assert!(pointer_target(&frames, 2, 3).is_err());
        assert!(pointer_target(&frames, -1, 0).is_err());
        assert!(pointer_target(&frames, 10, 0).is_err());
        assert!(compose(&[]).is_err());
    }
}
