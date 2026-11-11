use crate::error::AppError;
use libloading::{Library, Symbol};
use plugin_interface::{PROCESS_IMAGE_SYMBOL, ProcessImageFn};
use std::ffi::CStr;
use std::path::{Path, PathBuf};

/// Загруженная динамическая библиотека плагина.
///
/// `Library` хранится в структуре и живёт не меньше, чем любой `Symbol`,
/// полученный из неё, и не меньше вызова `process_image`. Если выгрузить
/// библиотеку раньше, вызов функции станет неопределённым поведением.
pub struct LoadedPlugin {
    library: Library,
    path: PathBuf,
}

impl LoadedPlugin {
    /// Ищет библиотеку в `directory` по голому имени плагина и загружает её.
    pub fn open(directory: &Path, plugin_name: &str) -> Result<Self, AppError> {
        let file_name = library_file_name(plugin_name)?;
        // components() заново собирает путь и на Windows заменяет смешанные
        // `/` и `\` на разделитель ОС. Иначе `target/debug` выглядит как `target/debug\mirror.dll`.
        let path: PathBuf = directory.join(file_name).components().collect();
        if !path.is_file() {
            return Err(AppError::PluginNotFound(path));
        }

        // SAFETY: путь указывает на существующий файл. Ошибку загрузки возвращаем вызывающему.
        let library = unsafe { Library::new(&path) }.map_err(|err| AppError::PluginLoad {
            path: path.clone(),
            message: err.to_string(),
        })?;

        Ok(Self { library, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Вызывает `process_image`. `params` обязан оставаться живым на всё время вызова.
    pub fn process(
        &self,
        width: u32,
        height: u32,
        rgba: &mut [u8],
        params: &CStr,
    ) -> Result<(), AppError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(AppError::DimensionsOverflow)?;
        if rgba.len() != expected {
            return Err(AppError::BufferSize {
                expected,
                actual: rgba.len(),
            });
        }

        // Символ живёт в этой функции дольше, чем сам вызов: библиотека при этом
        // заимствована через `&self` и не может быть выгружена.
        let process: Symbol<ProcessImageFn> = unsafe { self.library.get(PROCESS_IMAGE_SYMBOL) }
            .map_err(|err| AppError::SymbolNotFound {
                path: self.path.clone(),
                message: err.to_string(),
            })?;

        // SAFETY: буфер принадлежит хосту и не освобождается до возврата.
        // Длина проверена и равна width * height * 4. `params` — живая C-строка.
        unsafe {
            process(width, height, rgba.as_mut_ptr(), params.as_ptr());
        }
        Ok(())
    }
}

/// Собирает имя файла библиотеки по правилам текущей ОС.
///
/// Windows: `{name}.dll`, macOS: `lib{name}.dylib`, остальные Unix: `lib{name}.so`.
pub fn library_file_name(plugin_name: &str) -> Result<String, AppError> {
    validate_plugin_name(plugin_name)?;
    let file_name = if cfg!(windows) {
        format!("{plugin_name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{plugin_name}.dylib")
    } else {
        format!("lib{plugin_name}.so")
    };
    Ok(file_name)
}

fn validate_plugin_name(plugin_name: &str) -> Result<(), AppError> {
    let valid = !plugin_name.is_empty()
        && plugin_name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');
    if valid {
        Ok(())
    } else {
        Err(AppError::InvalidPluginName(plugin_name.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_name_follows_the_host_os() {
        let name = library_file_name("mirror").unwrap();
        if cfg!(windows) {
            assert_eq!(name, "mirror.dll");
        } else if cfg!(target_os = "macos") {
            assert_eq!(name, "libmirror.dylib");
        } else {
            assert_eq!(name, "libmirror.so");
        }

        let blur = library_file_name("blur").unwrap();
        if cfg!(windows) {
            assert_eq!(blur, "blur.dll");
        } else if cfg!(target_os = "macos") {
            assert_eq!(blur, "libblur.dylib");
        } else {
            assert_eq!(blur, "libblur.so");
        }
    }

    #[test]
    fn rejects_paths_prefixes_and_extensions() {
        for name in [
            "",
            "mirror.dll",
            "libmirror.so",
            "libmirror.dylib",
            "../mirror",
            "a/b",
            "a\\b",
        ] {
            assert!(library_file_name(name).is_err(), "{name}");
        }
    }
}
