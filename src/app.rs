use std::io;

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

/// アプリケーションモード
enum Mode {
    /// 通常モード（スクロール操作）
    Normal,
    /// 検索入力モード（/ キー押下後）
    Search,
    /// コマンド入力モード（: キー押下後）
    Command,
}

/// TUI アプリケーションの状態
pub struct App {
    /// プリレンダー済みの全行
    lines: Vec<Line<'static>>,
    /// ソースファイル名（ステータスバー表示用）
    filename: String,
    /// スクロールオフセット（表示開始行）
    scroll: usize,
    /// ビューポートの高さ（行数）
    viewport_height: usize,
    /// 現在のモード
    mode: Mode,
    /// 検索クエリ（入力中）
    search_input: String,
    /// 確定済みの検索クエリ
    search_query: String,
    /// マッチした行インデックス
    search_matches: Vec<usize>,
    /// 現在のマッチカーソル位置
    search_cursor: usize,
    /// コマンド入力（: キー押下後）
    command_input: String,
    /// 終了フラグ
    should_quit: bool,
}

impl App {
    pub fn new(lines: Vec<Line<'static>>, filename: String) -> Self {
        Self {
            lines,
            filename,
            scroll: 0,
            viewport_height: 24,
            mode: Mode::Normal,
            search_input: String::new(),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_cursor: 0,
            command_input: String::new(),
            should_quit: false,
        }
    }

    pub fn run(&mut self) -> Result<()> {
        // raw mode 中にパニックするとターミナルが壊れるため、hook で復旧を保証する
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stderr(), LeaveAlternateScreen);
            original_hook(info);
        }));

        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.event_loop(&mut terminal);

        // クリーンアップ（必ず実行）
        disable_raw_mode().ok();
        execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
        terminal.show_cursor().ok();

        result
    }

    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        loop {
            terminal.draw(|frame| {
                self.viewport_height = frame.area().height.saturating_sub(1) as usize;
                self.draw(frame);
            })?;

            match event::read()? {
                Event::Key(key) => self.handle_key(key),
                Event::Resize(_, _) => {} // viewport_height は次の draw で更新
                _ => {}
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Search => self.handle_search_key(key),
            Mode::Command => self.handle_command_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true
            }

            // 1行スクロール
            KeyCode::Char('j') | KeyCode::Down => self.scroll_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_by(-1),

            // ページスクロール
            KeyCode::Char(' ') | KeyCode::Char('f') | KeyCode::PageDown => {
                self.scroll_by(self.page_size() as isize)
            }
            KeyCode::Char('b') | KeyCode::PageUp => self.scroll_by(-(self.page_size() as isize)),

            // 半ページスクロール
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_by((self.viewport_height / 2) as isize)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_by(-((self.viewport_height / 2) as isize))
            }

            // 先頭/末尾
            KeyCode::Char('g') => self.scroll = 0,
            KeyCode::Char('G') => self.scroll = self.max_scroll(),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = self.max_scroll(),

            // 検索開始
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search_input.clear();
            }

            // コマンドモード
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_input.clear();
            }

            // 次/前のマッチ
            KeyCode::Char('n') => self.jump_next_match(),
            KeyCode::Char('N') => self.jump_prev_match(),

            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.search_query = self.search_input.clone();
                self.execute_search();
                self.mode = Mode::Normal;
                if !self.search_matches.is_empty() {
                    self.search_cursor = 0;
                    self.scroll = self.search_matches[0].min(self.max_scroll());
                }
            }
            KeyCode::Esc => {
                self.search_input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.search_input.pop();
            }
            KeyCode::Char(c) => {
                self.search_input.push(c);
            }
            _ => {}
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if let Ok(line_num) = self.command_input.parse::<usize>() {
                    // 1-indexed → 0-indexed にして行ジャンプ
                    self.scroll = line_num.saturating_sub(1).min(self.max_scroll());
                }
                self.command_input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.command_input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.command_input.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.command_input.push(c);
            }
            _ => {}
        }
    }

    fn draw(&self, frame: &mut ratatui::Frame) {
        let chunks =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(frame.area());

        // 可視範囲の行だけ取り出してハイライト適用
        let visible_end = (self.scroll + self.viewport_height).min(self.lines.len());
        let visible_lines: Vec<Line<'static>> = self.lines[self.scroll..visible_end]
            .iter()
            .map(|line| self.highlight_line(line))
            .collect();
        let content = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
        frame.render_widget(content, chunks[0]);

        // ステータスバー
        let status = self.build_status_bar(chunks[1].width);
        frame.render_widget(status, chunks[1]);
    }

    /// 検索クエリにマッチした部分を黄色背景でハイライトする
    fn highlight_line(&self, line: &Line<'static>) -> Line<'static> {
        if self.search_query.is_empty() {
            return line.clone();
        }

        // 全 Span のテキストを結合し、各 Span の範囲を記録
        let mut full_text = String::new();
        let mut span_ranges: Vec<(usize, usize, Style)> = Vec::new();
        for span in &line.spans {
            let start = full_text.len();
            full_text.push_str(&span.content);
            span_ranges.push((start, full_text.len(), span.style));
        }

        // マッチ位置を検出（大文字小文字を無視）
        let query_lower = self.search_query.to_lowercase();
        let text_lower = full_text.to_lowercase();
        let mut match_ranges: Vec<(usize, usize)> = Vec::new();
        let mut pos = 0;
        while let Some(found) = text_lower[pos..].find(&query_lower) {
            let abs = pos + found;
            match_ranges.push((abs, abs + query_lower.len()));
            pos = abs + query_lower.len();
        }

        if match_ranges.is_empty() {
            return line.clone();
        }

        let hl_style = Style::default().fg(Color::Black).bg(Color::Yellow);
        let mut new_spans: Vec<Span<'static>> = Vec::new();

        for &(sp_start, sp_end, original_style) in &span_ranges {
            let span_text = &full_text[sp_start..sp_end];
            let mut offset = 0;

            for &(m_start, m_end) in &match_ranges {
                if m_end <= sp_start || m_start >= sp_end {
                    continue;
                }
                // マッチ範囲をこの Span の境界にクランプ
                let hl_start = m_start.max(sp_start) - sp_start;
                let hl_end = m_end.min(sp_end) - sp_start;

                if hl_start > offset {
                    new_spans.push(Span::styled(
                        span_text[offset..hl_start].to_string(),
                        original_style,
                    ));
                }
                new_spans.push(Span::styled(
                    span_text[hl_start..hl_end].to_string(),
                    hl_style,
                ));
                offset = hl_end;
            }

            if offset < span_text.len() {
                new_spans.push(Span::styled(
                    span_text[offset..].to_string(),
                    original_style,
                ));
            }
        }

        Line::from(new_spans)
    }

    fn build_status_bar(&self, width: u16) -> Paragraph<'static> {
        let status_style = Style::default().fg(Color::Black).bg(Color::White);

        let pct_str = if self.lines.len() <= self.viewport_height {
            "All".to_string()
        } else if self.scroll == 0 {
            "Top".to_string()
        } else if self.scroll >= self.max_scroll() {
            "Bot".to_string()
        } else {
            format!("{}%", self.scroll_percent())
        };

        let left = match &self.mode {
            Mode::Search => format!("/{}", self.search_input),
            Mode::Command => format!(":{}", self.command_input),
            Mode::Normal => {
                if !self.search_query.is_empty() && !self.search_matches.is_empty() {
                    format!(
                        " {} [{}/{}]",
                        self.filename,
                        self.search_cursor + 1,
                        self.search_matches.len()
                    )
                } else if !self.search_query.is_empty() {
                    format!(" {} [not found]", self.filename)
                } else {
                    format!(" {}", self.filename)
                }
            }
        };

        let right = format!(" {} ", pct_str);
        let padding = (width as usize)
            .saturating_sub(left.width())
            .saturating_sub(right.width());

        let status_text = format!("{}{:padding$}{}", left, "", right, padding = padding);
        Paragraph::new(Line::styled(status_text, status_style))
    }

    fn scroll_by(&mut self, delta: isize) {
        let new = self.scroll as isize + delta;
        self.scroll = new.max(0) as usize;
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn max_scroll(&self) -> usize {
        self.lines
            .len()
            .saturating_sub(self.viewport_height)
            .min(u16::MAX as usize)
    }

    fn page_size(&self) -> usize {
        self.viewport_height.saturating_sub(2).max(1)
    }

    fn scroll_percent(&self) -> usize {
        let max = self.max_scroll();
        if max == 0 {
            100
        } else {
            (self.scroll * 100) / max
        }
    }

    fn execute_search(&mut self) {
        self.search_matches.clear();
        self.search_cursor = 0;

        if self.search_query.is_empty() {
            return;
        }

        let query_lower = self.search_query.to_lowercase();
        for (i, line) in self.lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            if text.to_lowercase().contains(&query_lower) {
                self.search_matches.push(i);
            }
        }
    }

    fn jump_next_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_cursor = (self.search_cursor + 1) % self.search_matches.len();
        self.scroll = self.search_matches[self.search_cursor].min(self.max_scroll());
    }

    fn jump_prev_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_cursor = if self.search_cursor == 0 {
            self.search_matches.len() - 1
        } else {
            self.search_cursor - 1
        };
        self.scroll = self.search_matches[self.search_cursor].min(self.max_scroll());
    }
}
