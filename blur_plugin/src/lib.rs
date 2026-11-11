//! Плагин размытия (box blur).
//!
//! Параметры: `radius` (радиус, целое без знака) и `iterations` (число проходов).
//! Отсутствующий ключ получает значение по умолчанию: радиус `1`, одна итерация.
//! Цвет пикселя заменяется средним арифметическим пикселей в квадрате
//! со стороной `2 * radius + 1`. Края кадра не дополняются: в среднее входят
//! только пиксели внутри изображения.

use plugin_interface::{KvError, parse_key_values, process_in_place};
use std::os::raw::c_char;

/// Параметры box blur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlurParams {
    pub radius: u32,
    pub iterations: u32,
}

impl Default for BlurParams {
    fn default() -> Self {
        Self {
            radius: 1,
            iterations: 1,
        }
    }
}

/// Ошибка разбора параметров размытия.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamError {
    Empty,
    InvalidSyntax(String),
    UnknownKey(String),
    DuplicateKey(String),
    InvalidValue { key: String, value: String },
}

impl std::fmt::Display for ParamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamError::Empty => write!(f, "пустая строка параметров"),
            ParamError::InvalidSyntax(entry) => write!(f, "некорректная запись `{entry}`"),
            ParamError::UnknownKey(key) => write!(f, "неизвестный параметр `{key}`"),
            ParamError::DuplicateKey(key) => write!(f, "параметр `{key}` указан дважды"),
            ParamError::InvalidValue { key, value } => write!(
                f,
                "некорректное значение `{value}` для `{key}`: ожидается целое число без знака"
            ),
        }
    }
}

impl std::error::Error for ParamError {}

impl From<KvError> for ParamError {
    fn from(err: KvError) -> Self {
        match err {
            KvError::Empty => ParamError::Empty,
            KvError::InvalidSyntax(entry) => ParamError::InvalidSyntax(entry),
        }
    }
}

/// Разбирает строку параметров. Пустая строка, неизвестные ключи и нечисловые
/// значения — ошибка. Отсутствующий ключ заменяется значением по умолчанию.
pub fn parse_params(input: &str) -> Result<BlurParams, ParamError> {
    let pairs = parse_key_values(input)?;
    let mut radius = None;
    let mut iterations = None;

    for (key, value) in pairs {
        match key.as_str() {
            "radius" => assign_u32(&mut radius, &key, &value)?,
            "iterations" => assign_u32(&mut iterations, &key, &value)?,
            _ => return Err(ParamError::UnknownKey(key)),
        }
    }

    Ok(BlurParams {
        radius: radius.unwrap_or(1),
        iterations: iterations.unwrap_or(1),
    })
}

fn assign_u32(slot: &mut Option<u32>, key: &str, value: &str) -> Result<(), ParamError> {
    if slot.is_some() {
        return Err(ParamError::DuplicateKey(key.to_owned()));
    }
    *slot = Some(parse_u32(key, value)?);
    Ok(())
}

fn parse_u32(key: &str, value: &str) -> Result<u32, ParamError> {
    value.parse::<u32>().map_err(|_| ParamError::InvalidValue {
        key: key.to_owned(),
        value: value.to_owned(),
    })
}

/// Применяет box blur. При неверной длине буфер не читается и не пишется.
pub fn apply_blur(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    radius: u32,
    iterations: u32,
) -> Result<(), String> {
    let expected = plugin_interface::rgba_byte_len(width, height)
        .ok_or_else(|| "размеры изображения вызывают переполнение".to_owned())?;
    if pixels.len() != expected {
        return Err(format!(
            "длина буфера {} не равна width * height * 4 = {expected}",
            pixels.len()
        ));
    }
    if width == 0 || height == 0 || radius == 0 || iterations == 0 {
        return Ok(());
    }

    let mut current = pixels.to_vec();
    let mut next = vec![0u8; pixels.len()];
    for _ in 0..iterations {
        box_blur_once(&current, &mut next, width, height, radius);
        std::mem::swap(&mut current, &mut next);
    }
    pixels.copy_from_slice(&current);
    Ok(())
}

fn box_blur_once(src: &[u8], dst: &mut [u8], width: u32, height: u32, radius: u32) {
    let width = width as usize;
    let height = height as usize;
    let radius = radius as usize;

    for y in 0..height {
        let y_start = y.saturating_sub(radius);
        let y_end = y.saturating_add(radius).min(height - 1);
        for x in 0..width {
            let x_start = x.saturating_sub(radius);
            let x_end = x.saturating_add(radius).min(width - 1);

            let mut sum = [0u64; 4];
            let mut count = 0u64;
            for yy in y_start..=y_end {
                for xx in x_start..=x_end {
                    let offset = (yy * width + xx) * 4;
                    for channel in 0..4 {
                        sum[channel] += u64::from(src[offset + channel]);
                    }
                    count += 1;
                }
            }

            let offset = (y * width + x) * 4;
            for channel in 0..4 {
                let average = sum[channel] / count;
                debug_assert!(average <= u64::from(u8::MAX));
                dst[offset + channel] = average as u8;
            }
        }
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)] // сигнатура задана C ABI, разыменование внутри process_in_place
pub extern "C" fn process_image(
    width: u32,
    height: u32,
    rgba_data: *mut u8,
    params: *const c_char,
) {
    // SAFETY: контракт хоста — валидные размеры, буфер RGBA и C-строка параметров.
    unsafe {
        process_in_place(
            width,
            height,
            rgba_data,
            params,
            "blur",
            |pixels, width, height, params| {
                let parsed = parse_params(params).map_err(|err| err.to_string())?;
                apply_blur(pixels, width, height, parsed.radius, parsed.iterations)
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn parses_valid_params() {
        assert_eq!(
            parse_params("radius=4,iterations=3").unwrap(),
            BlurParams {
                radius: 4,
                iterations: 3,
            }
        );
        assert_eq!(
            parse_params("radius = 0, iterations = 2").unwrap(),
            BlurParams {
                radius: 0,
                iterations: 2,
            }
        );
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        assert_eq!(
            parse_params("radius=4").unwrap(),
            BlurParams {
                radius: 4,
                iterations: 1,
            }
        );
        assert_eq!(
            parse_params("iterations=5").unwrap(),
            BlurParams {
                radius: 1,
                iterations: 5,
            }
        );
    }

    #[test]
    fn rejects_empty_string() {
        assert_eq!(parse_params(""), Err(ParamError::Empty));
        assert_eq!(parse_params("  \n"), Err(ParamError::Empty));
    }

    #[test]
    fn rejects_unknown_keys() {
        assert_eq!(
            parse_params("radius=4,sigma=1"),
            Err(ParamError::UnknownKey("sigma".to_owned()))
        );
        assert!(matches!(
            parse_params("amount=2"),
            Err(ParamError::UnknownKey(_))
        ));
    }

    #[test]
    fn rejects_non_numeric_values() {
        for sample in [
            "radius=abc",
            "radius=1.5",
            "iterations=-1",
            "radius=",
            "iterations=3px",
        ] {
            assert!(
                matches!(parse_params(sample), Err(ParamError::InvalidValue { .. })),
                "{sample}"
            );
        }
        assert!(matches!(
            parse_params("radius"),
            Err(ParamError::InvalidSyntax(_))
        ));
    }

    #[test]
    fn uniform_buffer_is_unchanged() {
        let pixel = [10u8, 20, 30, 40];
        let mut buf = pixel.repeat(3 * 3);
        let original = buf.clone();
        apply_blur(&mut buf, 3, 3, 4, 3).unwrap();
        assert_eq!(buf, original);
    }

    #[test]
    fn bright_center_pixel_spreads_to_neighbors() {
        let mut buf = vec![0u8; 3 * 3 * 4];
        let center = 4 * 4;
        buf[center] = 255;

        apply_blur(&mut buf, 3, 3, 1, 1).unwrap();

        for index in 0..9 {
            if index == 4 {
                continue;
            }
            let red = buf[index * 4];
            assert!(red > 0, "сосед {index} остался нулевым");
        }
        assert!(buf[center] > 0);
    }

    #[test]
    fn short_buffer_is_rejected_without_panic() {
        let mut buf = [0u8; 4];
        assert!(apply_blur(&mut buf, 2, 2, 1, 1).is_err());
        assert_eq!(buf, [0u8; 4]);
    }

    #[test]
    fn ffi_bright_pixel_writes_result_back() {
        let mut buf = vec![0u8; 3 * 3 * 4];
        buf[16] = 255;
        let params = CString::new("radius=1,iterations=1").unwrap();
        process_image(3, 3, buf.as_mut_ptr(), params.as_ptr());
        assert!(buf[0] > 0);
        assert!(buf[8] > 0);
    }

    #[test]
    fn ffi_invalid_params_leave_buffer_unchanged() {
        let mut buf = vec![7u8; 4];
        let original = buf.clone();
        for params in ["", "radius=nope", "unknown=1"] {
            let c_params = CString::new(params).unwrap();
            process_image(1, 1, buf.as_mut_ptr(), c_params.as_ptr());
            assert_eq!(buf, original, "{params}");
        }
    }
}
