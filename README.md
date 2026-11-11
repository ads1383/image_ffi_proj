# image_ffi_project

CLI-приложение загружает изображение, прогоняет его через динамический плагин и сохраняет результат в PNG.

## Сборка

```bash
cargo build --workspace
cargo test --workspace
```

Плагины собираются как `cdylib`:

| ОС | Файл в `target/debug` |
| --- | --- |
| Windows | `mirror.dll`, `blur.dll` |
| Linux | `libmirror.so`, `libblur.so` |
| macOS | `libmirror.dylib`, `libblur.dylib` |

## Запуск

Команды выполняются из корня репозитория. `--plugin` принимает голое имя без префикса и расширения. Каталог библиотек по умолчанию — `target/debug`.

```bash
# Зеркальный разворот
cargo run -p image_processor -- \
  --input input.png \
  --output out_mirror.png \
  --plugin mirror \
  --params params/mirror.txt

# Размытие
cargo run -p image_processor -- \
  --input input.png \
  --output out_blur.png \
  --plugin blur \
  --params params/blur.txt
```

Свой каталог плагинов:

```bash
cargo run -p image_processor -- \
  --input input.png \
  --output out_mirror.png \
  --plugin mirror \
  --params params/mirror.txt \
  --plugin-path target/debug
```

Без аргументов программа печатает справку и завершается. Ошибки (нет файла, битое изображение, плагин не найден) печатаются в stderr и не приводят к панике.

Сообщения о ходе работы идут через `log` / `env_logger`. Уровень по умолчанию — `info`. Его можно сменить переменной `RUST_LOG`, например `RUST_LOG=error`.

## Параметры плагинов

Текстовый файл: пары `ключ=значение`, разделённые запятой, точкой с запятой или переводом строки.

`mirror` — зеркальное отражение.

| Ключ | Значения | По умолчанию |
| --- | --- | --- |
| `horizontal` | `true` / `false` | `false` |
| `vertical` | `true` / `false` | `false` |

`blur` — box blur. Новый цвет пикселя равен среднему цветов в квадрате со стороной `2 * radius + 1`. Несколько итераций приближают результат к гауссову размытию.

| Ключ | Значения | По умолчанию |
| --- | --- | --- |
| `radius` | целое число без знака | `1` |
| `iterations` | целое число без знака | `1` |

Пустая строка, неизвестный ключ и некорректное значение — ошибка плагина: изображение не меняется, сообщение печатается в stderr. Отсутствующий ключ заменяется значением по умолчанию.

## Как устроен вызов плагина

Каждый плагин экспортирует

```rust
#[unsafe(no_mangle)]
pub extern "C" fn process_image(
    width: u32,
    height: u32,
    rgba_data: *mut u8,
    params: *const std::os::raw::c_char,
)
```

Хост передаёт ширину, высоту и плоский буфер RGBA8 (`width * height * 4` байт). Плагин меняет пиксели в этом же буфере и не освобождает его. Тело `process_image` обёрнуто в `std::panic::catch_unwind`: паника не пересекает границу FFI, буфер хоста при этом остаётся прежним.

Общая сигнатура вынесена в крейт `plugin_interface`, чтобы хост и плагины не разъехались.
