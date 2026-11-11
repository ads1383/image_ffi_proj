//! Плагин зеркального отражения.
//!
//! Параметры: `horizontal` и `vertical` (`true` или `false`).
//! Отсутствующий ключ получает значение по умолчанию `false`.

use plugin_interface::{KvError, parse_key_values, process_in_place};
use std::os::raw::c_char;

/// Параметры отражения. По умолчанию изображение не меняется.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MirrorParams {
    pub horizontal: bool,
    pub vertical: bool,
}

/// Ошибка разбора параметров зеркала.
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
                "некорректное значение `{value}` для `{key}`: ожидается true или false"
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

/// Разбирает строку параметров. Пустая строка, неизвестные ключи и значения
/// вне `true`/`false` — ошибка. Отсутствующий ключ заменяется значением по умолчанию.
pub fn parse_params(input: &str) -> Result<MirrorParams, ParamError> {
    let pairs = parse_key_values(input)?;
    let mut horizontal = None;
    let mut vertical = None;

    for (key, value) in pairs {
        match key.as_str() {
            "horizontal" => assign_bool(&mut horizontal, &key, &value)?,
            "vertical" => assign_bool(&mut vertical, &key, &value)?,
            _ => return Err(ParamError::UnknownKey(key)),
        }
    }

    Ok(MirrorParams {
        horizontal: horizontal.unwrap_or(false),
        vertical: vertical.unwrap_or(false),
    })
}

fn assign_bool(slot: &mut Option<bool>, key: &str, value: &str) -> Result<(), ParamError> {
    if slot.is_some() {
        return Err(ParamError::DuplicateKey(key.to_owned()));
    }
    *slot = Some(parse_bool(key, value)?);
    Ok(())
}

fn parse_bool(key: &str, value: &str) -> Result<bool, ParamError> {
    if value.eq_ignore_ascii_case("true") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("false") {
        Ok(false)
    } else {
        Err(ParamError::InvalidValue {
            key: key.to_owned(),
            value: value.to_owned(),
        })
    }
}

/// Отражает буфер RGBA8 на месте. При неверной длине буфер не читается и не пишется.
pub fn apply_mirror(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    horizontal: bool,
    vertical: bool,
) -> Result<(), String> {
    let expected = plugin_interface::rgba_byte_len(width, height)
        .ok_or_else(|| "размеры изображения вызывают переполнение".to_owned())?;
    if pixels.len() != expected {
        return Err(format!(
            "длина буфера {} не равна width * height * 4 = {expected}",
            pixels.len()
        ));
    }
    if width == 0 || height == 0 || (!horizontal && !vertical) {
        return Ok(());
    }

    let width = width as usize;
    let height = height as usize;

    if horizontal {
        for y in 0..height {
            for x in 0..width / 2 {
                swap_pixels(pixels, width, x, y, width - 1 - x, y);
            }
        }
    }
    if vertical {
        let row_bytes = width * 4;
        for y in 0..height / 2 {
            let top = y * row_bytes;
            let bottom = (height - 1 - y) * row_bytes;
            for offset in 0..row_bytes {
                pixels.swap(top + offset, bottom + offset);
            }
        }
    }
    Ok(())
}

fn swap_pixels(pixels: &mut [u8], width: usize, x0: usize, y0: usize, x1: usize, y1: usize) {
    let a = (y0 * width + x0) * 4;
    let b = (y1 * width + x1) * 4;
    for channel in 0..4 {
        pixels.swap(a + channel, b + channel);
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
            "mirror",
            |pixels, width, height, params| {
                let parsed = parse_params(params).map_err(|err| err.to_string())?;
                apply_mirror(pixels, width, height, parsed.horizontal, parsed.vertical)
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn sample_2x2() -> Vec<u8> {
        vec![
            1, 2, 3, 4, // (0, 0)
            5, 6, 7, 8, // (1, 0)
            9, 10, 11, 12, // (0, 1)
            13, 14, 15, 16, // (1, 1)
        ]
    }

    #[test]
    fn parses_valid_params() {
        assert_eq!(
            parse_params("horizontal=true,vertical=false").unwrap(),
            MirrorParams {
                horizontal: true,
                vertical: false,
            }
        );
        assert_eq!(
            parse_params("horizontal = false, vertical = true").unwrap(),
            MirrorParams {
                horizontal: false,
                vertical: true,
            }
        );
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        assert_eq!(
            parse_params("horizontal=true").unwrap(),
            MirrorParams {
                horizontal: true,
                vertical: false,
            }
        );
        assert_eq!(
            parse_params("vertical=true").unwrap(),
            MirrorParams {
                horizontal: false,
                vertical: true,
            }
        );
    }

    #[test]
    fn rejects_empty_string() {
        assert_eq!(parse_params(""), Err(ParamError::Empty));
        assert_eq!(parse_params("   \n"), Err(ParamError::Empty));
    }

    #[test]
    fn rejects_unknown_keys() {
        assert_eq!(
            parse_params("horizontal=true,rotate=90"),
            Err(ParamError::UnknownKey("rotate".to_owned()))
        );
        assert!(matches!(
            parse_params("foo=bar"),
            Err(ParamError::UnknownKey(_))
        ));
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(matches!(
            parse_params("horizontal=yes"),
            Err(ParamError::InvalidValue { .. })
        ));
        assert!(matches!(
            parse_params("vertical=1"),
            Err(ParamError::InvalidValue { .. })
        ));
        assert!(matches!(
            parse_params("horizontal="),
            Err(ParamError::InvalidValue { .. })
        ));
        assert!(matches!(
            parse_params("not-a-pair"),
            Err(ParamError::InvalidSyntax(_))
        ));
    }

    #[test]
    fn horizontal_flip_2x2() {
        let mut buf = sample_2x2();
        apply_mirror(&mut buf, 2, 2, true, false).unwrap();
        assert_eq!(
            buf,
            vec![5, 6, 7, 8, 1, 2, 3, 4, 13, 14, 15, 16, 9, 10, 11, 12]
        );
    }

    #[test]
    fn vertical_flip_2x2() {
        let mut buf = sample_2x2();
        apply_mirror(&mut buf, 2, 2, false, true).unwrap();
        assert_eq!(
            buf,
            vec![9, 10, 11, 12, 13, 14, 15, 16, 1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[test]
    fn horizontal_and_vertical_flip_2x2() {
        let mut buf = sample_2x2();
        apply_mirror(&mut buf, 2, 2, true, true).unwrap();
        assert_eq!(
            buf,
            vec![13, 14, 15, 16, 9, 10, 11, 12, 5, 6, 7, 8, 1, 2, 3, 4]
        );
    }

    #[test]
    fn short_buffer_is_rejected_without_panic() {
        let mut buf = [0u8; 4];
        assert!(apply_mirror(&mut buf, 2, 2, true, true).is_err());
        assert_eq!(buf, [0u8; 4]);
    }

    #[test]
    fn ffi_horizontal_flip_writes_result_back() {
        let mut buf = sample_2x2();
        let params = CString::new("horizontal=true,vertical=false").unwrap();
        process_image(2, 2, buf.as_mut_ptr(), params.as_ptr());
        assert_eq!(
            buf,
            vec![5, 6, 7, 8, 1, 2, 3, 4, 13, 14, 15, 16, 9, 10, 11, 12]
        );
    }

    #[test]
    fn ffi_invalid_params_leave_buffer_unchanged() {
        let mut buf = sample_2x2();
        let original = buf.clone();
        for params in ["", "horizontal=yes", "unknown=1"] {
            let c_params = CString::new(params).unwrap();
            process_image(2, 2, buf.as_mut_ptr(), c_params.as_ptr());
            assert_eq!(buf, original, "{params}");
        }
        process_image(2, 2, buf.as_mut_ptr(), std::ptr::null());
        assert_eq!(buf, original);
    }

    #[test]
    fn ffi_null_buffer_does_not_panic() {
        let params = CString::new("horizontal=true,vertical=true").unwrap();
        process_image(2, 2, std::ptr::null_mut(), params.as_ptr());
    }
}
