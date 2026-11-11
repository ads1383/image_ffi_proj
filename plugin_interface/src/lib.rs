//! Контракт между хостом и плагинами обработки изображений.
//!
//! Плагин — это `cdylib`, которая экспортирует `process_image`. Хост находит
//! этот символ через `libloading` и передаёт плоский буфер RGBA.

use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, UnwindSafe, catch_unwind};

/// Имя символа для `Library::get`. Завершающий NUL обязателен для libloading.
/// На macOS в бинарнике символ лежит как `_process_image`, но `dlsym` (и поэтому
/// `libloading`) ожидает имя без подчёркивания на всех платформах.
pub const PROCESS_IMAGE_SYMBOL: &[u8] = b"process_image\0";

/// C-совместимая сигнатура точки входа плагина.
pub type ProcessImageFn = unsafe extern "C" fn(u32, u32, *mut u8, *const c_char);

/// Число байт в буфере RGBA8 размера `width` на `height`.
pub fn rgba_byte_len(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)
}

/// Ошибка разбора списка `ключ=значение`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KvError {
    /// Пустая строка или строка только из разделителей.
    Empty,
    /// Фрагмент без `=`.
    InvalidSyntax(String),
}

impl std::fmt::Display for KvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KvError::Empty => write!(f, "пустая строка параметров"),
            KvError::InvalidSyntax(entry) => {
                write!(
                    f,
                    "некорректная запись параметров `{entry}`: ожидается key=value"
                )
            }
        }
    }
}

impl std::error::Error for KvError {}

/// Разбирает текст вида `key=value`, разделённый запятой, точкой с запятой или переводом строки.
///
/// Пробелы вокруг ключей, значений и разделителей отбрасываются. Пустые фрагменты
/// (например, запятая в конце строки) пропускаются. Полностью пустой ввод — ошибка:
/// отсутствующие ключи обрабатывает уже сам плагин и подставляет значения по умолчанию.
pub fn parse_key_values(input: &str) -> Result<Vec<(String, String)>, KvError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(KvError::Empty);
    }

    let mut pairs = Vec::new();
    for raw in trimmed.split([',', ';', '\n']) {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = entry.split_once('=') else {
            return Err(KvError::InvalidSyntax(entry.to_owned()));
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() {
            return Err(KvError::InvalidSyntax(entry.to_owned()));
        }
        pairs.push((key.to_owned(), value.to_owned()));
    }

    if pairs.is_empty() {
        return Err(KvError::Empty);
    }
    Ok(pairs)
}

/// Вызывает тело плагина так, чтобы паника и ошибка разбора не портили буфер хоста.
///
/// # Safety
///
/// * `rgba_data` либо не используется (если пикселей ноль), либо указывает на
///   `width * height * 4` байт, доступных для записи всё время вызова.
/// * `params` равен null или указывает на валидную NUL-terminated строку,
///   которая живёт всё время вызова.
///
/// Перехват паники стоит именно здесь, на стороне плагина: к моменту возврата
/// через FFI ловить её в хосте уже поздно.
pub unsafe fn process_in_place<F>(
    width: u32,
    height: u32,
    rgba_data: *mut u8,
    params: *const c_char,
    plugin_name: &str,
    body: F,
) where
    F: FnOnce(&mut [u8], u32, u32, &str) -> Result<(), String> + UnwindSafe,
{
    let Some(len) = rgba_byte_len(width, height) else {
        eprintln!("{plugin_name}: размеры изображения вызывают переполнение");
        return;
    };
    if len == 0 {
        return;
    }
    if rgba_data.is_null() {
        eprintln!("{plugin_name}: указатель на пиксели равен null");
        return;
    }

    let params_text = match unsafe { read_params(params) } {
        Ok(text) => text,
        Err(err) => {
            eprintln!("{plugin_name}: {err}");
            return;
        }
    };

    // Исходный буфер не трогаем, пока обработка не завершится успешно.
    // SAFETY: указатель не null, длина совпадает с контрактом хоста и проверена выше.
    let buffer = unsafe { std::slice::from_raw_parts_mut(rgba_data, len) };
    let mut working = buffer.to_vec();

    let result = catch_unwind(AssertUnwindSafe(|| {
        body(&mut working, width, height, &params_text)
    }));

    match result {
        Ok(Ok(())) => buffer.copy_from_slice(&working),
        Ok(Err(err)) => eprintln!("{plugin_name}: {err}"),
        Err(_) => {
            eprintln!("{plugin_name}: паника внутри плагина, изображение оставлено без изменений")
        }
    }
}

/// # Safety
///
/// `params` равен null или указывает на валидную NUL-terminated строку.
unsafe fn read_params(params: *const c_char) -> Result<String, String> {
    if params.is_null() {
        return Err("строка параметров равна null".to_owned());
    }
    // SAFETY: вызывающий код гарантирует валидность C-строки.
    let c_str = unsafe { std::ffi::CStr::from_ptr(params) };
    c_str
        .to_str()
        .map(str::to_owned)
        .map_err(|_| "параметры не являются корректной UTF-8 строкой".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_commas_and_newlines() {
        let pairs = parse_key_values(" horizontal = true , vertical = false\n").unwrap();
        assert_eq!(
            pairs,
            vec![
                ("horizontal".to_owned(), "true".to_owned()),
                ("vertical".to_owned(), "false".to_owned()),
            ]
        );
    }

    #[test]
    fn empty_input_is_an_error() {
        assert_eq!(parse_key_values(""), Err(KvError::Empty));
        assert_eq!(parse_key_values(" \n\t "), Err(KvError::Empty));
        assert_eq!(parse_key_values(",,,"), Err(KvError::Empty));
    }

    #[test]
    fn missing_equals_is_an_error() {
        assert!(matches!(
            parse_key_values("horizontal"),
            Err(KvError::InvalidSyntax(_))
        ));
    }
}
