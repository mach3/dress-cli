use comrak::nodes::{AlertType, NodeValue};
use comrak::{Arena, Options, parse_document};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use unicode_width::UnicodeWidthStr;

/// Markdown ソースをパースし、ratatui の Line ベクターに変換する
pub fn render_markdown(source: &str) -> Vec<Line<'static>> {
    let arena = Arena::new();
    let opts = comrak_options();
    let root = parse_document(&arena, source, &opts);

    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();

    let mut lines = Vec::new();
    walk_blocks(root, &ss, &ts, &mut lines, 0);
    lines
}

/// comrak のパースオプション（GFM 拡張有効化）
fn comrak_options<'a>() -> Options<'a> {
    let mut opts = Options::default();
    opts.extension.strikethrough = true;
    opts.extension.table = true;
    opts.extension.autolink = true;
    opts.extension.tasklist = true;
    opts.extension.alerts = true;
    opts
}

/// ブロックレベルの AST ノードを再帰的に走査して Line に変換する
fn walk_blocks<'a>(
    node: &'a comrak::nodes::AstNode<'a>,
    ss: &SyntaxSet,
    ts: &ThemeSet,
    lines: &mut Vec<Line<'static>>,
    list_depth: usize,
) {
    for child in node.children() {
        let data = child.data.borrow();
        match &data.value {
            NodeValue::Heading(heading) => {
                let style = heading_style(heading.level);
                let prefix = "#".repeat(heading.level as usize);
                let mut spans = vec![Span::styled(format!("{} ", prefix), style)];
                drop(data);
                collect_inline_spans(child, &mut spans, style);
                lines.push(Line::from(spans));
                lines.push(Line::raw(""));
            }

            NodeValue::Paragraph => {
                let style = Style::default();
                let mut spans = Vec::new();
                if list_depth > 0 {
                    let indent = "  ".repeat(list_depth);
                    spans.push(Span::raw(indent));
                }
                drop(data);
                collect_inline_spans(child, &mut spans, style);
                lines.push(Line::from(spans));
                lines.push(Line::raw(""));
            }

            NodeValue::CodeBlock(code_block) => {
                let lang = code_block.info.trim().to_string();
                let code = code_block.literal.clone();
                drop(data);

                let highlighted = highlight_code(&code, &lang, ss, ts);
                let border_style = Style::default().fg(Color::DarkGray);
                let label = if lang.is_empty() { "code" } else { &lang };
                // 上下の枠線幅を揃える
                let border_width: usize = 50;
                let top_pad = border_width.saturating_sub(label.len() + 4);
                lines.push(Line::from(Span::styled(
                    format!("┌─ {} {}", label, "─".repeat(top_pad)),
                    border_style,
                )));
                lines.extend(highlighted);
                lines.push(Line::from(Span::styled(
                    format!("└{}", "─".repeat(border_width - 1)),
                    border_style,
                )));
                lines.push(Line::raw(""));
            }

            NodeValue::List(list) => {
                let start = list.start;
                let is_ordered = list.list_type == comrak::nodes::ListType::Ordered;
                drop(data);
                render_list_items(child, lines, list_depth, is_ordered, start);
                lines.push(Line::raw(""));
            }

            NodeValue::Alert(alert) => {
                let alert_type = alert.alert_type;
                let title = alert
                    .title
                    .clone()
                    .unwrap_or_else(|| alert_type.default_title());
                drop(data);

                let color = alert_color(alert_type);
                let icon = alert_icon(alert_type);
                let title_style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                lines.push(Line::from(Span::styled(
                    format!("{} {}", icon, title),
                    title_style,
                )));

                let mut inner_lines = Vec::new();
                walk_blocks(child, ss, ts, &mut inner_lines, list_depth);
                let bar_style = Style::default().fg(color);
                for line in inner_lines {
                    let mut spans = vec![Span::styled("│ ", bar_style)];
                    spans.extend(
                        line.spans
                            .into_iter()
                            .map(|s| Span::styled(s.content.into_owned(), s.style.fg(color))),
                    );
                    lines.push(Line::from(spans));
                }
                lines.push(Line::raw(""));
            }

            NodeValue::BlockQuote => {
                drop(data);
                let mut inner_lines = Vec::new();
                walk_blocks(child, ss, ts, &mut inner_lines, list_depth);
                let quote_style = Style::default().fg(Color::DarkGray);
                for line in inner_lines {
                    let mut spans = vec![Span::styled("│ ", quote_style)];
                    spans.extend(line.spans.into_iter().map(|s| {
                        Span::styled(s.content.into_owned(), s.style.fg(Color::DarkGray))
                    }));
                    lines.push(Line::from(spans));
                }
                lines.push(Line::raw(""));
            }

            NodeValue::ThematicBreak => {
                drop(data);
                lines.push(Line::from(Span::styled(
                    "─".repeat(60),
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::raw(""));
            }

            NodeValue::Table(_) => {
                drop(data);
                render_table(child, lines);
                lines.push(Line::raw(""));
            }

            NodeValue::FrontMatter(_) | NodeValue::HtmlBlock(_) => {
                drop(data);
            }

            _ => {
                drop(data);
                walk_blocks(child, ss, ts, lines, list_depth);
            }
        }
    }
}

/// リストの子 Item を番号付きで描画する
fn render_list_items<'a>(
    list_node: &'a comrak::nodes::AstNode<'a>,
    lines: &mut Vec<Line<'static>>,
    list_depth: usize,
    is_ordered: bool,
    start: usize,
) {
    let indent = "  ".repeat(list_depth);
    let bullet_style = Style::default().fg(Color::Cyan);

    for (idx, item_node) in list_node.children().enumerate() {
        let bullet = if is_ordered {
            format!("{}{}. ", indent, start + idx)
        } else {
            format!("{}• ", indent)
        };

        // タスクリスト: Item の最初の子の TaskItem をチェック
        let mut task_prefix = None;
        for item_child in item_node.children() {
            let item_data = item_child.data.borrow();
            if let NodeValue::TaskItem(checked) = &item_data.value {
                task_prefix = Some(if checked.is_some() {
                    format!("{}[x] ", indent)
                } else {
                    format!("{}[ ] ", indent)
                });
                break;
            }
        }

        let actual_bullet = task_prefix.unwrap_or(bullet);
        let mut item_spans = vec![Span::styled(actual_bullet, bullet_style)];

        for item_child in item_node.children() {
            let item_data = item_child.data.borrow();
            match &item_data.value {
                NodeValue::Paragraph => {
                    drop(item_data);
                    collect_inline_spans(item_child, &mut item_spans, Style::default());
                    lines.push(Line::from(item_spans.clone()));
                    item_spans = Vec::new();
                }
                NodeValue::List(nested_list) => {
                    let nested_start = nested_list.start;
                    let nested_ordered = nested_list.list_type == comrak::nodes::ListType::Ordered;
                    if !item_spans.is_empty() {
                        lines.push(Line::from(item_spans.clone()));
                        item_spans = Vec::new();
                    }
                    drop(item_data);
                    render_list_items(
                        item_child,
                        lines,
                        list_depth + 1,
                        nested_ordered,
                        nested_start,
                    );
                }
                NodeValue::TaskItem(_) => {
                    drop(item_data);
                }
                _ => {
                    drop(item_data);
                }
            }
        }
        if !item_spans.is_empty() {
            lines.push(Line::from(item_spans));
        }
    }
}

/// インライン要素を Span のベクターに収集する
fn collect_inline_spans<'a>(
    node: &'a comrak::nodes::AstNode<'a>,
    spans: &mut Vec<Span<'static>>,
    base_style: Style,
) {
    for child in node.children() {
        let data = child.data.borrow();
        match &data.value {
            NodeValue::Text(text) => {
                spans.push(Span::styled(text.clone(), base_style));
            }

            NodeValue::Code(code) => {
                spans.push(Span::styled(
                    format!("`{}`", code.literal),
                    base_style.fg(Color::Yellow).add_modifier(Modifier::DIM),
                ));
            }

            NodeValue::Emph => {
                let style = base_style.add_modifier(Modifier::ITALIC);
                drop(data);
                collect_inline_spans(child, spans, style);
                continue;
            }

            NodeValue::Strong => {
                let style = base_style.add_modifier(Modifier::BOLD);
                drop(data);
                collect_inline_spans(child, spans, style);
                continue;
            }

            NodeValue::Strikethrough => {
                let style = base_style
                    .add_modifier(Modifier::CROSSED_OUT)
                    .fg(Color::DarkGray);
                drop(data);
                collect_inline_spans(child, spans, style);
                continue;
            }

            NodeValue::Link(link) => {
                let url = link.url.clone();
                let link_style = base_style
                    .fg(Color::Blue)
                    .add_modifier(Modifier::UNDERLINED);
                drop(data);
                collect_inline_spans(child, spans, link_style);
                spans.push(Span::styled(
                    format!(" ({})", url),
                    Style::default().fg(Color::DarkGray),
                ));
                continue;
            }

            NodeValue::Image(image) => {
                let url = image.url.clone();
                drop(data);
                // alt テキストは子ノードから収集
                let mut alt_spans = Vec::new();
                collect_inline_spans(child, &mut alt_spans, Style::default());
                let alt_text: String = alt_spans.iter().map(|s| s.content.as_ref()).collect();
                let label = if alt_text.is_empty() { &url } else { &alt_text };
                spans.push(Span::styled(
                    format!("[image: {}]", label),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ));
                continue;
            }

            NodeValue::SoftBreak => {
                spans.push(Span::raw(" "));
            }

            NodeValue::LineBreak => {
                spans.push(Span::raw(" "));
            }

            NodeValue::TaskItem(_) => {
                // タスクマーカーはスキップ（Item レベルで処理済み）
            }

            _ => {
                drop(data);
                collect_inline_spans(child, spans, base_style);
                continue;
            }
        }
    }
}

/// 見出しレベルに応じたスタイルを返す
fn heading_style(level: u8) -> Style {
    match level {
        1 => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        2 => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        3 => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        4 => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().add_modifier(Modifier::BOLD),
    }
}

/// アラート種別に対応する色を返す
fn alert_color(alert_type: AlertType) -> Color {
    match alert_type {
        AlertType::Note => Color::Blue,
        AlertType::Tip => Color::Green,
        AlertType::Important => Color::Magenta,
        AlertType::Warning => Color::Yellow,
        AlertType::Caution => Color::Red,
    }
}

/// アラート種別に対応するアイコンを返す
fn alert_icon(alert_type: AlertType) -> &'static str {
    match alert_type {
        AlertType::Note => "ℹ",
        AlertType::Tip => "💡",
        AlertType::Important => "❗",
        AlertType::Warning => "⚠",
        AlertType::Caution => "🔴",
    }
}

/// syntect でコードブロックをハイライトする
fn highlight_code(code: &str, lang: &str, ss: &SyntaxSet, ts: &ThemeSet) -> Vec<Line<'static>> {
    let syntax = if lang.is_empty() {
        ss.find_syntax_plain_text()
    } else {
        ss.find_syntax_by_token(lang)
            .unwrap_or_else(|| ss.find_syntax_plain_text())
    };

    let theme = &ts.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);

    LinesWithEndings::from(code)
        .filter_map(|line| {
            let ranges = highlighter.highlight_line(line, ss).ok()?;
            let spans: Vec<Span<'static>> = ranges
                .into_iter()
                .map(|(style, text)| {
                    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                    Span::styled(
                        text.trim_end_matches('\n').to_string(),
                        Style::default().fg(fg),
                    )
                })
                .collect();
            Some(Line::from(spans))
        })
        .collect()
}

/// テーブルを Line ベクターに変換する
fn render_table<'a>(node: &'a comrak::nodes::AstNode<'a>, lines: &mut Vec<Line<'static>>) {
    let header_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let border_style = Style::default().fg(Color::DarkGray);

    // セル内容を収集
    let mut rows: Vec<(bool, Vec<String>)> = Vec::new();

    for row_node in node.children() {
        let row_data = row_node.data.borrow();
        let is_header = matches!(&row_data.value, NodeValue::TableRow(h) if *h);
        drop(row_data);

        let mut row = Vec::new();
        for cell_node in row_node.children() {
            let mut cell_text = String::new();
            collect_cell_text(cell_node, &mut cell_text);
            row.push(cell_text);
        }
        rows.push((is_header, row));
    }

    if rows.is_empty() {
        return;
    }

    // 列幅を計算
    let num_cols = rows.iter().map(|(_, r)| r.len()).max().unwrap_or(0);
    let mut col_widths = vec![0usize; num_cols];
    for (_, row) in &rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                col_widths[i] = col_widths[i].max(cell.width());
            }
        }
    }

    for (is_header, row) in &rows {
        let style = if *is_header {
            header_style
        } else {
            Style::default()
        };
        let mut spans = vec![Span::styled("│ ", border_style)];
        for (i, cell) in row.iter().enumerate() {
            let col_w = col_widths.get(i).copied().unwrap_or(0);
            // 表示幅ベースでスペースを手動パディング
            let padding = col_w.saturating_sub(cell.width());
            spans.push(Span::styled(
                format!("{}{}", cell, " ".repeat(padding)),
                style,
            ));
            spans.push(Span::styled(" │ ", border_style));
        }
        lines.push(Line::from(spans));

        // ヘッダー行の後にセパレータ
        if *is_header {
            let mut sep = vec![Span::styled("├─", border_style)];
            for (i, &w) in col_widths.iter().enumerate() {
                sep.push(Span::styled("─".repeat(w), border_style));
                if i < num_cols - 1 {
                    sep.push(Span::styled("─┼─", border_style));
                }
            }
            sep.push(Span::styled("─┤", border_style));
            lines.push(Line::from(sep));
        }
    }
}

/// テーブルセル内のテキストを再帰的に収集する
fn collect_cell_text<'a>(node: &'a comrak::nodes::AstNode<'a>, out: &mut String) {
    for child in node.children() {
        let data = child.data.borrow();
        match &data.value {
            NodeValue::Text(text) => {
                out.push_str(text);
            }
            NodeValue::Code(code) => {
                out.push('`');
                out.push_str(&code.literal);
                out.push('`');
            }
            NodeValue::SoftBreak | NodeValue::LineBreak => {
                out.push(' ');
            }
            _ => {
                drop(data);
                collect_cell_text(child, out);
                continue;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading_renders() {
        let lines = render_markdown("# Hello World");
        assert!(!lines.is_empty());
        let first_line_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first_line_text.contains("Hello World"));
    }

    #[test]
    fn test_code_block_renders() {
        let md = "```rust\nfn main() {}\n```";
        let lines = render_markdown(md);
        // 枠線上 + コード行 + 枠線下 + 空行
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_list_renders() {
        let md = "- item 1\n- item 2\n- item 3";
        let lines = render_markdown(md);
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_table_renders() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let lines = render_markdown(md);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_alert_note_renders() {
        let md = "> [!NOTE]\n> This is a note.";
        let lines = render_markdown(md);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("Note"), "should contain alert title");
        assert!(all_text.contains("This is a note."), "should contain body");
    }

    #[test]
    fn test_alert_warning_renders() {
        let md = "> [!WARNING]\n> Be careful.";
        let lines = render_markdown(md);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("Warning"));
    }
}
