//! Хост динамических плагинов обработки изображений.

mod error;
mod plugin_loader;

pub use error::AppError;
pub use plugin_loader::{LoadedPlugin, library_file_name};

use std::ffi::CString;
use std::path::PathBuf;

/// Аргументы одного запуска обработки.
#[derive(Debug)]
pub struct Options {
    pub input: PathBuf,
    pub output: PathBuf,
    pub plugin: String,
    pub params: PathBuf,
    pub plugin_path: PathBuf,
}

/// Загружает изображение, прогоняет его через плагин и сохраняет PNG.
pub fn run(options: &Options) -> Result<(), AppError> {
    // Имя проверяем до обращения к диску, чтобы опечатка вроде `mirror.dll` была явной.
    let _ = library_file_name(&options.plugin)?;

    if !options.input.is_file() {
        return Err(AppError::InputNotFound(options.input.clone()));
    }
    if !options.params.is_file() {
        return Err(AppError::ParamsNotFound(options.params.clone()));
    }

    log::info!("загрузка изображения {}", options.input.display());
    let image = image::open(&options.input)?;
    let mut rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    log::info!("размер изображения: {width}x{height}");

    let params_text = std::fs::read_to_string(&options.params)?;
    // CString хранится в отдельной переменной до конца вызова плагина.
    // Выражение `CString::new(text)?.as_ptr()` уничтожило бы строку сразу
    // и оставило бы плагину висячий указатель.
    let params = CString::new(params_text).map_err(|_| AppError::ParamsContainNul)?;

    let plugin = LoadedPlugin::open(&options.plugin_path, &options.plugin)?;
    log::info!("плагин загружен: {}", plugin.path().display());

    let pixels: &mut [u8] = &mut rgba;
    plugin.process(width, height, pixels, &params)?;

    log::info!("сохранение PNG {}", options.output.display());
    image::DynamicImage::ImageRgba8(rgba)
        .save_with_format(&options.output, image::ImageFormat::Png)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("image_ffi_proj_{label}_{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_input_file_is_reported() {
        let err = run(&Options {
            input: PathBuf::from("this-file-does-not-exist.png"),
            output: PathBuf::from("out.png"),
            plugin: "mirror".into(),
            params: PathBuf::from("also-missing.txt"),
            plugin_path: PathBuf::from("target/debug"),
        })
        .unwrap_err();
        assert!(matches!(err, AppError::InputNotFound(_)));
    }

    #[test]
    fn missing_params_file_is_reported() {
        let dir = scratch_dir("params");
        let input = dir.join("input.png");
        fs::write(&input, b"placeholder").unwrap();
        let err = run(&Options {
            input,
            output: dir.join("out.png"),
            plugin: "mirror".into(),
            params: dir.join("missing-params.txt"),
            plugin_path: PathBuf::from("target/debug"),
        })
        .unwrap_err();
        assert!(matches!(err, AppError::ParamsNotFound(_)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_plugin_name_is_rejected() {
        let err = run(&Options {
            input: PathBuf::from("input.png"),
            output: PathBuf::from("out.png"),
            plugin: "mirror.dll".into(),
            params: PathBuf::from("params.txt"),
            plugin_path: PathBuf::from("target/debug"),
        })
        .unwrap_err();
        assert!(matches!(err, AppError::InvalidPluginName(_)));
    }

    #[test]
    fn corrupt_image_is_an_error() {
        let dir = scratch_dir("corrupt");
        let input = dir.join("bad.png");
        let params = dir.join("params.txt");
        fs::write(&input, b"this is not a png").unwrap();
        fs::write(&params, b"horizontal=true,vertical=false").unwrap();

        let err = run(&Options {
            input,
            output: dir.join("out.png"),
            plugin: "mirror".into(),
            params,
            plugin_path: PathBuf::from("target/debug"),
        })
        .unwrap_err();
        assert!(matches!(err, AppError::Image(_)), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_plugin_library_is_reported() {
        let dir = scratch_dir("plugin");
        let input = dir.join("in.png");
        let params = dir.join("params.txt");
        let plugin_dir = dir.join("plugins");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(&params, b"horizontal=true,vertical=false").unwrap();

        let image: image::RgbaImage =
            image::ImageBuffer::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
        image.save(Path::new(&input)).unwrap();

        let err = run(&Options {
            input,
            output: dir.join("out.png"),
            plugin: "mirror".into(),
            params,
            plugin_path: plugin_dir,
        })
        .unwrap_err();
        assert!(matches!(err, AppError::PluginNotFound(_)), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }
}
