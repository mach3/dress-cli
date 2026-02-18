mod app;
mod render;

use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

/// dress — ターミナルで Markdown を美しく表示するページャー
#[derive(Parser, Debug)]
#[command(name = "dress", version, about)]
struct Args {
    /// 表示する Markdown ファイル（省略時は stdin から読み込み）
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let (source, filename) = match args.file {
        Some(path) => {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("ファイルを読めへん: {}", path.display()))?;
            (content, path.display().to_string())
        }
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("stdin の読み込みに失敗")?;
            (buf, "<stdin>".to_string())
        }
    };

    let lines = render::render_markdown(&source);
    let mut app = app::App::new(lines, filename);
    app.run()
}
