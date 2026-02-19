# dress

Markdown をターミナルで美しく表示する CLI ツール。
ページング表示と vim キーバインドによるナビゲーションをサポート。

## Features

- **GitHub Flavored Markdown** - テーブル、タスクリスト、取り消し線、オートリンク
- **シンタックスハイライト** - コードブロックの言語別ハイライト
- **vim キーバインド** - `j`/`k`、`g`/`G`、`/` 検索、`:` 行ジャンプ
- **検索ハイライト** - マッチしたテキストを黄色背景で表示
- **ファイル / stdin** - ファイル指定またはパイプ入力に対応

## Install

### Cargo

```sh
cargo install dress-cli
```

### Shell script (macOS / Linux)

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mach3/dress-cli/releases/latest/download/dress-cli-installer.sh | sh
```

### Build from source

```sh
git clone https://github.com/mach3/dress-cli.git
cd dress-cli
cargo install --path .
```

## Usage

```sh
# ファイルを指定して表示
dress README.md

# パイプで渡す
cat README.md | dress
```

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `k` | 1 行スクロール |
| `Space` / `b` | ページスクロール |
| `Ctrl+d` / `Ctrl+u` | 半ページスクロール |
| `g` / `G` | 先頭 / 末尾 |
| `/` | 検索 |
| `n` / `N` | 次 / 前のマッチ |
| `:行番号` | 指定行にジャンプ |
| `q` | 終了 |

## License

MIT
