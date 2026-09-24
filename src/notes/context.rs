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

#[derive(Debug)]
pub struct ContextOutput {
    /// 古い日付が先。
    pub days: Vec<(NaiveDate, String)>,
    pub estimated_tokens: usize,
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
    let cutoff = today - Duration::days(i64::from(since_days.max(1)) - 1);
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
