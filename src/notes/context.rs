use std::path::Path;

use chrono::{Duration, NaiveDate};

use super::note_path;

/// 日本語・英語混在を想定した安全側(少なめ)の概算。実際のモデルの
/// トークナイザとは一致しない。1トークンあたり平均2文字と仮定する
/// (英語は1トークン≒4文字、日本語のCJKは1トークン≒1〜2文字になりやすく、
/// その中間よりやや安全側に寄せた値)。
fn estimate_tokens(s: &str) -> usize {
    s.chars().count().div_ceil(2)
}

/// `pen context` と MCP の `recent_notes` が共有する既定値。どちらかだけ
/// 変わると、同じ「直近のノート」が入口によって違う範囲になるので1箇所に書く。
pub const CONTEXT_DEFAULT_DAYS: u32 = 7;
pub const CONTEXT_DEFAULT_MAX_TOKENS: usize = 4000;

/// 遡る日数の上限(約100年)。1日1ファイルを1日ずつ stat していくので、
/// 上限が無いと `--since 4000000000d` のような値で日付の計算があふれて
/// panic し、あふれない範囲でも事実上終わらなくなる。人が書くノートの期間と
/// しては十分に長い。
const MAX_CONTEXT_DAYS: u32 = 36_600;

#[derive(Debug)]
pub struct ContextOutput {
    /// 古い日付が先。
    pub days: Vec<(NaiveDate, String)>,
    pub estimated_tokens: usize,
}

impl ContextOutput {
    /// LLM にそのまま渡せるマークダウン。日ごとに `# YYYY-MM-DD` の見出しを付け、
    /// 末尾に概算トークン数をコメントで添える。`pen context` と MCP の
    /// `recent_notes` が同じ形で返すよう、ここにだけ書く。
    pub fn to_markdown(&self, max_tokens: usize) -> String {
        if self.days.is_empty() {
            return "no notes in range".to_string();
        }
        let mut sections: Vec<String> = self
            .days
            .iter()
            .map(|(date, content)| format!("# {date}\n{content}"))
            .collect();
        sections.push(format!(
            "<!-- estimated tokens: {} / budget: {max_tokens} -->",
            self.estimated_tokens
        ));
        sections.join("\n")
    }
}

/// `today` から `since_days` 日分(当日を含む)のノートを、新しい日から
/// 遡って `max_tokens` の概算予算に収まるだけ集める。ファイルの途中では
/// 切らない——1日単位で入れるか入れないかを決める。ただし直近1日だけで
/// 既に予算を超える場合は、空を返すより実用的なのでその1日だけ返す。
pub fn context(
    notes_dir: &Path,
    since_days: u32,
    max_tokens: usize,
    today: NaiveDate,
) -> ContextOutput {
    let since_days = since_days.clamp(1, MAX_CONTEXT_DAYS);
    // 上限を掛けてもなお、`today` 自体が表現できる最古の日付に近ければ
    // あふれうるので、その場合は最古の日付まで遡る。
    let cutoff = today
        .checked_sub_signed(Duration::days(i64::from(since_days) - 1))
        .unwrap_or(NaiveDate::MIN);
    let mut collected = Vec::new();
    let mut total_tokens = 0;
    let mut date = today;
    loop {
        if date < cutoff {
            break;
        }
        if let Ok(contents) = std::fs::read_to_string(note_path(notes_dir, date)) {
            let tokens = estimate_tokens(&contents);
            if !collected.is_empty() && total_tokens + tokens > max_tokens {
                break;
            }
            collected.push((date, contents));
            total_tokens += tokens;
            if total_tokens > max_tokens {
                break;
            }
        }
        match date.pred_opt() {
            Some(prev) => date = prev,
            None => break,
        }
    }
    collected.reverse();
    ContextOutput {
        days: collected,
        estimated_tokens: total_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::tests::write_note;

    #[test]
    fn estimate_tokens_rounds_up_half_a_token() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("ab"), 1);
        assert_eq!(estimate_tokens("abc"), 2);
        assert_eq!(estimate_tokens("日本語"), 2);
    }

    #[test]
    fn context_collects_days_within_range_oldest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        write_note(tmp.path(), today, "today");
        write_note(tmp.path(), today.pred_opt().unwrap(), "yesterday");
        let out_of_range = today - Duration::days(5);
        write_note(tmp.path(), out_of_range, "too old");

        let out = context(tmp.path(), 3, 10_000, today);

        assert_eq!(out.days.len(), 2);
        assert_eq!(out.days[0].0, today.pred_opt().unwrap());
        assert_eq!(out.days[1].0, today);
    }

    #[test]
    fn context_skips_days_with_no_note() {
        let tmp = tempfile::tempdir().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        write_note(tmp.path(), today, "today");
        // today - 1 は意図的に書かない。
        write_note(tmp.path(), today - Duration::days(2), "two days ago");

        let out = context(tmp.path(), 3, 10_000, today);

        assert_eq!(out.days.len(), 2);
        assert_eq!(out.days[0].0, today - Duration::days(2));
        assert_eq!(out.days[1].0, today);
    }

    #[test]
    fn context_stops_at_a_day_boundary_when_budget_would_be_exceeded() {
        let tmp = tempfile::tempdir().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        // "today" 単体で概算5トークン、"yesterday" を足すと超える予算にする。
        write_note(tmp.path(), today, "0123456789"); // 10 chars -> 5 tokens
        write_note(tmp.path(), today.pred_opt().unwrap(), "0123456789");

        let out = context(tmp.path(), 7, 6, today);

        assert_eq!(out.days.len(), 1);
        assert_eq!(out.days[0].0, today);
        assert_eq!(out.estimated_tokens, 5);
    }

    #[test]
    fn context_with_a_huge_range_does_not_overflow() {
        let tmp = tempfile::tempdir().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        write_note(tmp.path(), today, "today");

        let out = context(tmp.path(), u32::MAX, 4000, today);

        assert_eq!(out.days.len(), 1);
    }

    #[test]
    fn context_near_the_earliest_representable_date_does_not_overflow() {
        let tmp = tempfile::tempdir().unwrap();

        let out = context(tmp.path(), 30, 4000, NaiveDate::MIN);

        assert!(out.days.is_empty());
    }

    #[test]
    fn context_returns_a_single_day_even_if_it_alone_exceeds_the_budget() {
        let tmp = tempfile::tempdir().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
        write_note(tmp.path(), today, "0123456789"); // 10 chars -> 5 tokens

        let out = context(tmp.path(), 7, 1, today);

        assert_eq!(out.days.len(), 1);
        assert_eq!(out.days[0].0, today);
        assert_eq!(out.estimated_tokens, 5);
    }
}
