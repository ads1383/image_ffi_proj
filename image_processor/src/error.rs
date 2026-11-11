use std::path::PathBuf;

/// Ошибка приложения. Все сбои файлов, декодирования и загрузки плагина
/// возвращаются сюда и печатаются в `main`, без паники.
#[derive(Debug)]
pub enum AppError {
    InputNotFound(PathBuf),
    ParamsNotFound(PathBuf),
    PluginNotFound(PathBuf),
    InvalidPluginName(String),
    ParamsContainNul,
    DimensionsOverflow,
    BufferSize { expected: usize, actual: usize },
    Image(image::ImageError),
    Io(std::io::Error),
    PluginLoad { path: PathBuf, message: String },
    SymbolNotFound { path: PathBuf, message: String },
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::InputNotFound(path) => {
                write!(f, "входной файл не найден: {}", path.display())
            }
            AppError::ParamsNotFound(path) => {
                write!(f, "файл параметров не найден: {}", path.display())
            }
            AppError::PluginNotFound(path) => write!(
                f,
                "библиотека плагина не найдена: {} (ожидается файл по правилам ОС: mirror.dll, libmirror.so или libmirror.dylib)",
                path.display()
            ),
            AppError::InvalidPluginName(name) => write!(
                f,
                "некорректное имя плагина `{name}`: укажите голое имя без пути, префикса и расширения, например mirror"
            ),
            AppError::ParamsContainNul => write!(
                f,
                "файл параметров содержит нулевой байт и не может быть передан в плагин как C-строка"
            ),
            AppError::DimensionsOverflow => {
                write!(f, "ширина и высота изображения вызывают переполнение")
            }
            AppError::BufferSize { expected, actual } => write!(
                f,
                "некорректный размер буфера RGBA: ожидалось {expected} байт, получено {actual}"
            ),
            AppError::Image(err) => write!(f, "не удалось обработать изображение: {err}"),
            AppError::Io(err) => write!(f, "ошибка ввода-вывода: {err}"),
            AppError::PluginLoad { path, message } => {
                write!(
                    f,
                    "не удалось загрузить плагин {}: {message}",
                    path.display()
                )
            }
            AppError::SymbolNotFound { path, message } => write!(
                f,
                "в библиотеке {} нет функции process_image: {message}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::Image(err) => Some(err),
            AppError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<image::ImageError> for AppError {
    fn from(err: image::ImageError) -> Self {
        AppError::Image(err)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err)
    }
}
