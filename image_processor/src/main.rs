use clap::Parser;
use image_processor::{Options, run};
use std::path::PathBuf;
use std::process::ExitCode;

/// Загружает изображение, обрабатывает его плагином и сохраняет результат в PNG.
#[derive(Parser, Debug)]
#[command(
    name = "image_processor",
    version,
    about,
    arg_required_else_help = true
)]
struct Cli {
    /// Путь к исходному изображению.
    #[arg(long)]
    input: PathBuf,

    /// Путь, по которому будет сохранён обработанный PNG.
    #[arg(long)]
    output: PathBuf,

    /// Имя плагина без префикса и расширения (например, mirror).
    #[arg(long)]
    plugin: String,

    /// Путь к текстовому файлу с параметрами обработки.
    #[arg(long)]
    params: PathBuf,

    /// Каталог, в котором лежит динамическая библиотека плагина.
    #[arg(long, default_value = "target/debug")]
    plugin_path: PathBuf,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();
    let options = Options {
        input: cli.input,
        output: cli.output,
        plugin: cli.plugin,
        params: cli.params,
        plugin_path: cli.plugin_path,
    };

    if let Err(err) = run(&options) {
        eprintln!("Ошибка: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
