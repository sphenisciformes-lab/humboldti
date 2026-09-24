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
    /// 結果一覧の表示開始位置。表示できる行数は描画時にしか分からないので、
    /// `draw_results` が選択行が見えるように調整する。
    pub offset: usize,
    pub error: Option<String>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            offset: 0,
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

/// `help` はキーの案内(`KeyMap::help_items` から組み立てる)。
pub fn draw_input(frame: &mut Frame, state: &SearchState, help: &str) {
    let area = frame.area();
    let [input_area, message_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);

    let title = bordered_title(&format!("Search ({help})"), input_area.width);
    let block = Block::bordered().title(title);
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

/// 前回の表示開始位置をできるだけ保ったまま、選択行が見える位置に調整する。
/// 毎回選択行を先頭や末尾に合わせ直すと、j/k のたびに一覧全体が動いて
/// 目で追いにくい。端末を広げたときに末尾の下が空かないよう、`len` でも抑える。
fn scroll_offset(offset: usize, selected: usize, height: usize, len: usize) -> usize {
    if height == 0 {
        return offset;
    }
    let offset = if selected < offset {
        selected
    } else if selected >= offset + height {
        selected + 1 - height
    } else {
        offset
    };
    offset.min(len.saturating_sub(height))
}

/// 枠線の上に載るタイトルを、枠の内側の幅に収まるよう切り詰める。
/// 検索クエリや案内が長いと、そのままでは枠からはみ出して切れる。
fn bordered_title(title: &str, outer_width: u16) -> String {
    width::truncate(title, outer_width.saturating_sub(2) as usize)
}

/// `help` はキーの案内(`KeyMap::help_items` から組み立てる)。
pub fn draw_results(frame: &mut Frame, state: &mut SearchState, help: &str) {
    let area = frame.area();
    let title = format!(
        "Search results for \"{}\" ({}) — {help}",
        state.query,
        state.results.len()
    );
    let block = Block::bordered().title(bordered_title(&title, area.width));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    if state.results.is_empty() {
        frame.render_widget(Paragraph::new("no matches"), inner);
        return;
    }

    let height = inner.height as usize;
    state.offset = scroll_offset(state.offset, state.selected, height, state.results.len());

    let lines: Vec<Line> = state
        .results
        .iter()
        .enumerate()
        .skip(state.offset)
        .take(height)
        .map(|(i, hit)| {
            let prefix = width::pad(&format!("{} L{}", hit.date, hit.line_number), 20);
            let remaining = (inner.width as usize).saturating_sub(width::width(&prefix));
            let content = width::truncate(&width::expand_tabs(&hit.line), remaining);
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

        terminal
            .draw(|frame| draw_results(frame, &mut state, ""))
            .unwrap();

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
    fn scroll_offset_keeps_the_selection_visible_and_moves_as_little_as_possible() {
        // 表示範囲内の移動では動かない。
        assert_eq!(scroll_offset(0, 4, 5, 20), 0);
        // 下端を越えたら、選択行が最下行に来る分だけ進む。
        assert_eq!(scroll_offset(0, 5, 5, 20), 1);
        // 戻るときは、上端を越えるまで動かない。
        assert_eq!(scroll_offset(3, 5, 5, 20), 3);
        assert_eq!(scroll_offset(3, 2, 5, 20), 2);
        // 端末が広がって末尾の下が空くなら、その分だけ戻す。
        assert_eq!(scroll_offset(15, 19, 10, 20), 10);
        // 全件が収まるなら先頭から。
        assert_eq!(scroll_offset(2, 2, 10, 3), 0);
        // 描画領域が無いときは触らない。
        assert_eq!(scroll_offset(3, 7, 0, 20), 3);
    }

    #[test]
    fn results_list_scrolls_to_keep_the_selection_on_screen() {
        // 枠線を除いた結果一覧の高さは 3 行。
        let backend = TestBackend::new(40, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = SearchState::new();
        state.query = "x".to_string();
        state.results = (1..=10)
            .map(|day| hit(&format!("2026-08-{day:02}"), 1, "x"))
            .collect();
        for _ in 0..6 {
            state.select_next();
        }

        terminal
            .draw(|frame| draw_results(frame, &mut state, ""))
            .unwrap();

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
        assert!(
            content.contains("2026-08-07"),
            "選択中の7件目が画面外に出ている"
        );
        assert!(!content.contains("2026-08-01"));
    }

    #[test]
    fn empty_results_show_a_placeholder() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = SearchState::new();
        state.query = "nothing-matches-this".to_string();

        terminal
            .draw(|frame| draw_results(frame, &mut state, ""))
            .unwrap();

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
