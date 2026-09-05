use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

// 幅の判定に迷ったときに常に wide 側を選ぶ既定値。ambiguous_width 設定
// （Config 導入後に配線予定）が無いv0.0時点ではこれを固定で使う。
const ELLIPSIS: &str = "…";
const ELLIPSIS_WIDTH: usize = 1;

/// 端末の桁数での表示幅。
pub fn width(s: &str) -> usize {
    s.graphemes(true).map(grapheme_width).sum()
}

/// 最大 `max` 桁に切り詰める。切った場合は `…` を付ける。
/// grapheme cluster を分割しない。
pub fn truncate(s: &str, max: usize) -> String {
    if width(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }

    let budget = max - ELLIPSIS_WIDTH;
    let mut result = String::new();
    let mut running = 0;
    for g in s.graphemes(true) {
        let w = grapheme_width(g);
        if running + w > budget {
            break;
        }
        result.push_str(g);
        running += w;
    }
    result.push_str(ELLIPSIS);
    result
}

/// ちょうど `target` 桁になるよう空白で埋める。長すぎる場合は切り詰める。
pub fn pad(s: &str, target: usize) -> String {
    let mut base = if width(s) > target {
        truncate(s, target)
    } else {
        s.to_string()
    };
    let w = width(&base);
    if w < target {
        base.push_str(&" ".repeat(target - w));
    }
    base
}

/// 各行が最大 `max` 桁になるよう折り返す。
pub fn wrap(s: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for line in s.split('\n') {
        let mut current = String::new();
        let mut running = 0;
        for g in line.graphemes(true) {
            let w = grapheme_width(g);
            if running > 0 && running + w > max {
                lines.push(std::mem::take(&mut current));
                running = 0;
            }
            current.push_str(g);
            running += w;
        }
        lines.push(current);
    }
    lines
}

// grapheme cluster の幅は、構成する char の幅の合計ではなく最大値を取る。
// 結合文字や ZWJ 絵文字シーケンス（例: 👨‍👩‍👧）は複数 char から成るが、
// 端末には 1 グリフとして描画されるため。
//
// 省略記号だけは特別扱いする。U+2026 は East Asian Width Ambiguous に
// 分類され、width_cjk（既定の wide 解釈）では2桁になる。しかしこれは
// ユーザーの文章ではなく自分たちが描画する UI 要素であり、ambiguous_width
// 設定の対象外として常に1桁で扱う。
fn grapheme_width(g: &str) -> usize {
    if g == ELLIPSIS {
        return ELLIPSIS_WIDTH;
    }
    g.chars()
        .map(|c| c.width_cjk().unwrap_or(0))
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_cases() {
        assert_eq!(width("abc"), 3);
        assert_eq!(width("日本語"), 6);
        assert_eq!(width("a日b"), 4);
        assert_eq!(width(""), 0);
    }

    #[test]
    fn truncate_never_splits_a_wide_char() {
        // "日本語" を 5 桁で切ると、半端な文字ではなく
        // 4 桁ぶんの文字 + 省略記号になること。
        assert_eq!(width(&truncate("日本語", 5)), 5);
    }

    #[test]
    fn pad_reaches_exact_width() {
        assert_eq!(width(&pad("日本", 10)), 10);
        assert_eq!(width(&pad("日本語日本語", 6)), 6);
    }

    #[test]
    fn wrap_breaks_at_max_width() {
        let lines = wrap("日本語日本語", 4);
        assert_eq!(lines, vec!["日本", "語日", "本語"]);
        for line in &lines {
            assert!(width(line) <= 4);
        }
    }
}
