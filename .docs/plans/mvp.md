# Implementation Plan: dress-cli MVP

## Feature Overview

dress-cli は、ターミナル上で Markdown を美しく描画する CLI ツール。ファイル引数または stdin から Markdown を読み込み、ratatui ベースの TUI でページング表示する。vim キーバインド（j/k/Space/b/g/G///n/N/q）で操作し、コードブロックはシンタックスハイライト付きで表示する。

## Selected Architecture

**案A: ミニマル（4ファイル構成）** を採用。

- `main.rs` — エントリポイント、clap 引数パース、ターミナルセットアップ
- `app.rs` — App 状態、イベントループ、vim キーバインド、描画
- `render.rs` — comrak AST → `Vec<Line<'static>>` コンパイラ（syntect 統合含む）
- `error.rs` — AppError enum（thiserror）

**選定理由**: 個人プロジェクトでシンプルさ優先。中間表現なしで直接変換。必要になったら後から分割可能。

## File Change Plan

すべて新規作成:

| ファイル | 行数目安 | 責務 |
|---------|---------|------|
| `Cargo.toml` | ~30 | 依存関係定義 |
| `src/main.rs` | ~40 | エントリポイント、CLI 引数、入力読み込み |
| `src/error.rs` | ~20 | エラー型定義 |
| `src/render.rs` | ~250 | Markdown パース + スタイル付きライン生成 |
| `src/app.rs` | ~300 | TUI アプリケーション全体 |

## Implementation Sequence

### Phase 1: プロジェクトスキャフォールド
1. `cargo init` でプロジェクト作成
2. `Cargo.toml` に依存関係を記述
3. `src/error.rs` を作成
4. `cargo check` で依存解決を確認

### Phase 2: Markdown レンダラー
1. `src/render.rs` を作成
2. comrak パース（GFM オプション有効化）
3. AST ウォーカー実装（heading, paragraph, code, list, etc.）
4. syntect によるコードブロックハイライト
5. 全要素を `Vec<Line<'static>>` に変換

### Phase 3: TUI アプリケーション
1. `src/app.rs` を作成
2. App 構造体（スクロール状態、検索状態）
3. `draw()` メソッド（Paragraph + ステータスバー）
4. キーバインド処理（j/k/Space/b/g/G/q）
5. `run()` メソッド（ターミナルセットアップ/ティアダウン + イベントループ）

### Phase 4: 検索機能
1. `/` キーで検索入力モード開始
2. 検索クエリのインクリメンタル入力
3. Enter で検索確定、マッチ行にジャンプ
4. n/N で次/前のマッチへ移動
5. Esc でキャンセル

### Phase 5: エントリポイント + 仕上げ
1. `src/main.rs` を作成（clap + 入力読み込み + app 起動）
2. パニック時のターミナル復旧処理
3. `cargo clippy` + `cargo fmt`

## Test Plan

- 最低限（Constitution の方針に従う）
- `render.rs`: 基本的な Markdown パースのユニットテスト（行数確認）
- 手動テスト: 実際の Markdown ファイルで動作確認

## Risks and Mitigation

| リスク | 対策 |
|-------|------|
| syntect の起動コスト | `load_defaults_newlines()` は組み込みバイナリで高速（<5ms）。遅い場合は lazy 初期化を検討 |
| comrak と ratatui のバージョン互換 | 最新安定版を使用。Cargo.lock でピン留め |
| 大きなファイルでのパフォーマンス | 一度だけプリレンダーし、表示は Vec スライスのみ。フレームごとのアロケーションなし |
| パニック時のターミナル復旧 | panic hook でraw mode解除 + alternate screen離脱を保証 |
| crossterm と ratatui のバージョン不整合 | ratatui 0.29 のデフォルト crossterm 対応バージョンに合わせる |
