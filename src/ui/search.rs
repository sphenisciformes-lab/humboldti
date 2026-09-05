use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};

use crate::notes::SearchHit;
use crate::width;

pub struct SearchState {
    pub query: String,
    pub results: Vec<SearchHit>,
    pub selected: usize,
    pub error: Option<String>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            error: None,
        }
    }

    pub fn select_next(&mut self) {
        if self.results.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.results.len() - 1);
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_hit(&self) -> Option<&SearchHit> {
        self.results.get(self.selected)
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw_input(frame: &mut Frame, state: &SearchState) {
    let area = frame.area();
    let [input_area, message_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);

    let block = Block::bordered().title("Search (Enter: search, Esc: cancel)");
    let inner = block.inner(input_area);
    let text = format!("/{}", state.query);
    frame.render_widget(Paragraph::new(text.clone()).block(block), input_area);

    // ここは実際にテキストを入力する場所なので、末尾にカーソルを明示する
    // (カレンダーのマス目とは違い、入力欄として見せたい)。
    frame.set_cursor_position((inner.x + width::width(&text) as u16, inner.y));

    if let Some(error) = &state.error {
        frame.render_widget(Paragraph::new(error.as_str()), message_area);
    }
}

pub fn draw_results(frame: &mut Frame, state: &SearchState) {
    let area = frame.area();
    let title = format!(
        "Search results for \"{}\" ({}) — j/k: move, Enter: open, q/Esc: back",
        state.query,
        state.results.len()
    );
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    if state.results.is_empty() {
        frame.render_widget(Paragraph::new("no matches"), inner);
        return;
    }

    let lines: Vec<Line> = state
        .results
        .iter()
        .enumerate()
        .take(inner.height as usize)
        .map(|(i, hit)| {
            let prefix = width::pad(&format!("{} L{}", hit.date, hit.line_number), 20);
            let remaining = (inner.width as usize).saturating_sub(width::width(&prefix));
            let content = width::truncate(&hit.line, remaining);
            let text = format!("{prefix}{content}");
            if i == state.selected {
                Line::styled(text, Style::default().add_modifier(Modifier::REVERSED))
            } else {
                Line::raw(text)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::SearchHit;
    use chrono::NaiveDate;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    fn hit(date: &str, line_number: usize, line: &str) -> SearchHit {
        SearchHit {
            path: PathBuf::from("/notes/dummy.md"),
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            line_number,
            line: line.to_string(),
        }
    }

    #[test]
    fn select_next_and_prev_do_not_panic_on_empty_results() {
        let mut state = SearchState::new();
        state.select_next();
        state.select_prev();
        assert_eq!(state.selected, 0);
        assert!(state.selected_hit().is_none());
    }

    #[test]
    fn select_next_and_prev_clamp_at_bounds() {
        let mut state = SearchState::new();
        state.results = vec![hit("2026-08-01", 1, "one"), hit("2026-08-02", 1, "two")];

        state.select_prev();
        assert_eq!(state.selected, 0);

        state.select_next();
        state.select_next();
        state.select_next();
        assert_eq!(state.selected, 1);
        assert_eq!(state.selected_hit().unwrap().line, "two");
    }

    #[test]
    fn results_list_truncates_long_lines_to_terminal_width() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = SearchState::new();
        state.query = "token".to_string();
        let long_line = "a very long line that should be truncated to fit the pane";
        state.results = vec![hit("2026-08-30", 3, long_line)];

        terminal.draw(|frame| draw_results(frame, &state)).unwrap();

        let content =
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .fold(String::new(), |mut acc, cell| {
                    acc.push_str(cell.symbol());
                    acc
                });
        assert!(content.contains("2026-08-30"));
        assert!(
            !content.contains(long_line),
            "40桁の端末に収まらない長さの行がそのまま描画されている"
        );
    }

    #[test]
    fn empty_results_show_a_placeholder() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = SearchState::new();
        state.query = "nothing-matches-this".to_string();

        terminal.draw(|frame| draw_results(frame, &state)).unwrap();

        let content =
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .fold(String::new(), |mut acc, cell| {
                    acc.push_str(cell.symbol());
                    acc
                });
        assert!(content.contains("no matches"));
    }
}
