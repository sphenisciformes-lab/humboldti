use std::path::Path;

use chrono::{Datelike, Duration, NaiveDate};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::action::Action;
use crate::notes;
use crate::width;

/// この幅未満ならプレビューペインを落とし、グリッドだけを表示する。
const MIN_WIDTH_FOR_PREVIEW: u16 = 100;
const CELL_WIDTH: u16 = 6;
const WEEKDAY_HEADERS: [&str; 7] = ["日", "月", "火", "水", "木", "金", "土"];

pub struct CalendarState {
    pub selected: NaiveDate,
}

pub enum CalendarRequest {
    Open(NaiveDate),
    Search,
    Quit,
}

impl CalendarState {
    pub fn new(today: NaiveDate) -> Self {
        Self { selected: today }
    }

    /// `Action` を受けて状態を進める。エディタを開く/終了するなど
    /// カレンダー画面だけでは完結しない要求は `CalendarRequest` として返す。
    pub fn apply(&mut self, action: Action) -> Option<CalendarRequest> {
        match action {
            Action::PrevDay => {
                self.selected -= Duration::days(1);
                None
            }
            Action::NextDay => {
                self.selected += Duration::days(1);
                None
            }
            Action::PrevWeek => {
                self.selected -= Duration::days(7);
                None
            }
            Action::NextWeek => {
                self.selected += Duration::days(7);
                None
            }
            Action::PrevMonth => {
                self.selected = shift_month(self.selected, -1);
                None
            }
            Action::NextMonth => {
                self.selected = shift_month(self.selected, 1);
                None
            }
            Action::PrevYear => {
                self.selected = shift_year(self.selected, -1);
                None
            }
            Action::NextYear => {
                self.selected = shift_year(self.selected, 1);
                None
            }
            Action::Open => Some(CalendarRequest::Open(self.selected)),
            Action::Quit => Some(CalendarRequest::Quit),
            Action::EnterSearch => Some(CalendarRequest::Search),
            // 検索の文字入力・結果移動系のアクションは resolve() が
            // Mode::Calendar では生成しないので、ここには渡ってこない想定。
            Action::InputChar(_)
            | Action::Backspace
            | Action::Confirm
            | Action::Cancel
            | Action::NextResult
            | Action::PrevResult => None,
        }
    }
}

/// 月初日を基準に `delta` ヶ月ずらす。対象月に存在しない日(例: 1/31 の
/// 1ヶ月後は 2/31 が存在しない)なら、その月の末日にまるめる。
fn shift_month(date: NaiveDate, delta: i32) -> NaiveDate {
    let total_months = date.year() * 12 + (date.month() as i32 - 1) + delta;
    let year = total_months.div_euclid(12);
    let month = total_months.rem_euclid(12) as u32 + 1;
    let day = date.day().min(days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, day).expect("year/month/clamped day is always valid")
}

/// 年だけ `delta` ずらす。うるう年の 2/29 から非うるう年に移るときなど、
/// 対象年に存在しない日ならその月の末日にまるめる。
fn shift_year(date: NaiveDate, delta: i32) -> NaiveDate {
    let year = date.year() + delta;
    let day = date.day().min(days_in_month(year, date.month()));
    NaiveDate::from_ymd_opt(year, date.month(), day).expect("year/clamped day is always valid")
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let this_month_start =
        NaiveDate::from_ymd_opt(year, month, 1).expect("valid year/month always has a 1st");
    let next_month_start = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("valid year/month always has a 1st");
    (next_month_start - this_month_start).num_days() as u32
}

/// `selected` を含む月を、日曜始まり6行7列のグリッドとして並べる。
/// 前後の月にはみ出した分もマス目を埋めるために含む。
fn month_grid(selected: NaiveDate) -> [[NaiveDate; 7]; 6] {
    let month_start = NaiveDate::from_ymd_opt(selected.year(), selected.month(), 1)
        .expect("year/month from an existing NaiveDate is always valid");
    let leading = month_start.weekday().num_days_from_sunday() as i64;
    let grid_start = month_start - Duration::days(leading);

    let mut grid = [[grid_start; 7]; 6];
    for (row, week) in grid.iter_mut().enumerate() {
        for (col, cell) in week.iter_mut().enumerate() {
            *cell = grid_start + Duration::days((row * 7 + col) as i64);
        }
    }
    grid
}

/// 固定 RGB ではなく端末の ANSI パレットに寄せる。こうすると利用者のテーマ
/// (ライト/ダーク)にそのまま追従するが、代わりに濃淡は2段階までしか
/// 区別できない — ANSI に「濃い緑」と「明るい緑」の2階調しかないため。
///
/// 文字色は端末の既定値に任せない。既定の前景色は「無地の背景」との組み
/// 合わせでしか読みやすさが保証されておらず、濃い緑の背景に重ねると
/// テーマによってはほぼ同化して読めなくなる。背景の明るさに応じて文字色を
/// 明示することで、これを両テーマで避ける。
fn density_bucket(size: u64) -> (Color, Option<Color>) {
    match size {
        0 => (Color::Reset, None),
        1..=799 => (Color::Green, Some(Color::White)),
        _ => (Color::LightGreen, Some(Color::Black)),
    }
}

fn density_style(notes_dir: &Path, date: NaiveDate) -> Style {
    let size = std::fs::metadata(notes::note_path(notes_dir, date))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let (bg, fg) = density_bucket(size);
    match fg {
        Some(fg) => Style::default().bg(bg).fg(fg),
        None => Style::default().bg(bg),
    }
}

const HELP_TEXT: &str =
    "hjkl/arrows: day/week   [ ]: month   { }: year   /: search   Enter: open   q/Esc: quit";

pub fn draw(frame: &mut Frame, state: &CalendarState, notes_dir: &Path) {
    let area = frame.area();
    let [main_area, help_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    let show_preview = main_area.width >= MIN_WIDTH_FOR_PREVIEW;
    let grid_area = if show_preview {
        let [grid_area, preview_area] =
            Layout::horizontal([Constraint::Length(CELL_WIDTH * 7 + 2), Constraint::Fill(1)])
                .areas(main_area);
        draw_preview(frame, preview_area, state, notes_dir);
        grid_area
    } else {
        main_area
    };

    draw_grid(frame, grid_area, state, notes_dir);
    frame.render_widget(Paragraph::new(HELP_TEXT), help_area);
}

fn draw_grid(frame: &mut Frame, area: Rect, state: &CalendarState, notes_dir: &Path) {
    let block = Block::bordered().title(state.selected.format("%Y-%m").to_string());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let header_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    let header: Vec<Span> = WEEKDAY_HEADERS
        .iter()
        .map(|d| Span::raw(width::pad(d, CELL_WIDTH as usize)))
        .collect();
    frame.render_widget(Paragraph::new(Line::from(header)), header_area);

    let grid = month_grid(state.selected);
    for (row_idx, week) in grid.iter().enumerate() {
        let y = inner.y + 1 + row_idx as u16;
        if y >= inner.y + inner.height {
            break;
        }
        for (col_idx, date) in week.iter().enumerate() {
            let x = inner.x + col_idx as u16 * CELL_WIDTH;
            if x >= inner.x + inner.width {
                break;
            }
            let cell_area = Rect {
                x,
                y,
                width: CELL_WIDTH.min(inner.x + inner.width - x),
                height: 1,
            };
            draw_cell(frame, cell_area, *date, state, notes_dir);
        }
    }
}

fn draw_cell(
    frame: &mut Frame,
    area: Rect,
    date: NaiveDate,
    state: &CalendarState,
    notes_dir: &Path,
) {
    let label = width::pad(&date.day().to_string(), CELL_WIDTH as usize);
    let mut style = density_style(notes_dir, date);
    if date.month() != state.selected.month() {
        style = style.fg(Color::DarkGray);
    }
    if date == state.selected {
        style = style.add_modifier(Modifier::REVERSED);
    }
    frame.render_widget(Paragraph::new(label).style(style), area);
}

fn draw_preview(frame: &mut Frame, area: Rect, state: &CalendarState, notes_dir: &Path) {
    let path = notes::note_path(notes_dir, state.selected);
    let content = std::fs::read_to_string(&path).unwrap_or_default();

    let block = Block::bordered().title(state.selected.format("%Y-%m-%d").to_string());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let text = if content.is_empty() {
        "no notes for this day".to_string()
    } else {
        content
    };
    let lines: Vec<Line> = width::wrap(&text, inner.width as usize)
        .into_iter()
        .take(inner.height as usize)
        .map(Line::raw)
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn month_grid_starts_on_sunday_and_covers_six_weeks() {
        let selected = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();
        let grid = month_grid(selected);

        for week in &grid {
            assert_eq!(week[0].weekday(), chrono::Weekday::Sun);
        }
        assert!(grid[0][0] <= NaiveDate::from_ymd_opt(2026, 8, 1).unwrap());
        assert!(grid[5][6] >= NaiveDate::from_ymd_opt(2026, 8, 31).unwrap());
    }

    #[test]
    fn apply_moves_selection_by_day_and_week() {
        let mut state = CalendarState::new(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());

        assert!(state.apply(Action::NextDay).is_none());
        assert_eq!(state.selected, NaiveDate::from_ymd_opt(2026, 3, 2).unwrap());

        assert!(state.apply(Action::PrevDay).is_none());
        assert!(state.apply(Action::PrevDay).is_none());
        assert_eq!(
            state.selected,
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );

        assert!(state.apply(Action::NextWeek).is_none());
        assert_eq!(state.selected, NaiveDate::from_ymd_opt(2026, 3, 7).unwrap());

        assert!(state.apply(Action::PrevWeek).is_none());
        assert_eq!(
            state.selected,
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );
    }

    #[test]
    fn apply_moves_selection_by_month_and_year() {
        let mut state = CalendarState::new(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());

        assert!(state.apply(Action::NextMonth).is_none());
        assert_eq!(
            state.selected,
            NaiveDate::from_ymd_opt(2026, 4, 15).unwrap()
        );

        assert!(state.apply(Action::PrevMonth).is_none());
        assert!(state.apply(Action::PrevMonth).is_none());
        assert_eq!(
            state.selected,
            NaiveDate::from_ymd_opt(2026, 2, 15).unwrap()
        );

        assert!(state.apply(Action::NextYear).is_none());
        assert_eq!(
            state.selected,
            NaiveDate::from_ymd_opt(2027, 2, 15).unwrap()
        );

        assert!(state.apply(Action::PrevYear).is_none());
        assert_eq!(
            state.selected,
            NaiveDate::from_ymd_opt(2026, 2, 15).unwrap()
        );
    }

    #[test]
    fn shift_month_clamps_to_the_last_valid_day() {
        // 1/31 の1ヶ月後は 2/31 が存在しないので 2/28(平年)に丸める。
        let jan_31 = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        assert_eq!(
            shift_month(jan_31, 1),
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );
    }

    #[test]
    fn shift_year_clamps_leap_day_to_february_end() {
        // うるう年の 2/29 から非うるう年に移ると 2/29 が存在しないので
        // 2/28 に丸める。
        let leap_day = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        assert_eq!(
            shift_year(leap_day, 1),
            NaiveDate::from_ymd_opt(2025, 2, 28).unwrap()
        );
    }

    #[test]
    fn apply_open_and_quit_bubble_up_as_requests() {
        let mut state = CalendarState::new(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());

        match state.apply(Action::Open) {
            Some(CalendarRequest::Open(date)) => assert_eq!(date, state.selected),
            _ => panic!("expected CalendarRequest::Open"),
        }
        assert!(matches!(
            state.apply(Action::Quit),
            Some(CalendarRequest::Quit)
        ));
    }

    #[test]
    fn density_bucket_increases_with_size() {
        assert_eq!(density_bucket(0), (Color::Reset, None));

        let (light_bg, light_fg) = density_bucket(1);
        let (heavy_bg, heavy_fg) = density_bucket(1000);
        assert_ne!(light_bg, Color::Reset);
        assert_ne!(light_bg, heavy_bg);

        // 濃い方の背景には白文字、明るい方には黒文字。背景の明るさに
        // 反する文字色にはならないこと(端末既定値に頼って同化するのを防ぐ)。
        assert_eq!(light_fg, Some(Color::White));
        assert_eq!(heavy_fg, Some(Color::Black));
    }

    #[test]
    fn weekday_header_fits_within_grid_width() {
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = CalendarState::new(NaiveDate::from_ymd_opt(2026, 8, 15).unwrap());
        let tmp = tempfile::tempdir().unwrap();

        terminal
            .draw(|frame| draw(frame, &state, tmp.path()))
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
        for header in WEEKDAY_HEADERS {
            assert!(content.contains(header));
        }
    }

    #[test]
    fn preview_pane_is_hidden_below_min_width() {
        let backend = TestBackend::new(MIN_WIDTH_FOR_PREVIEW - 1, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = CalendarState::new(NaiveDate::from_ymd_opt(2026, 8, 15).unwrap());
        let tmp = tempfile::tempdir().unwrap();

        terminal
            .draw(|frame| draw(frame, &state, tmp.path()))
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
        assert!(!content.contains("no notes for this day"));
    }
}
