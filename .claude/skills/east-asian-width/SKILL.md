---
name: east-asian-width
description: Humboldti Note における端末の表示幅の正しい扱い方。日本語・絵文字・結合文字を含みうる文字列の幅計算、切り詰め、パディング、折り返しの手順を示す。端末に描画するコード、カーソル位置の計算、カラムの整列、セルのパディング、罫線の描画、ラベルの切り詰め、行の折り返しを書くときや、レビューするときには必ずこのスキルを読むこと。レイアウトがずれている、日本語を入れると崩れるという報告を受けたときにも使う。「たぶん ASCII しか来ない」と思えるコードでも読むこと。このプロジェクトの文字列は既定で日本語であり、ほぼすべてが該当する。
---

# 端末の表示幅

このプロジェクトで最も頻出するバグの種類。以下の症状はすべて同じ原因に行き着く。
文字列の長さを画面上の幅として扱っている。

## 原則

`.len()` はバイト数。`.chars().count()` はスカラー値の数。どちらも表示幅ではない。

```
"日本語"        3 chars, 9 bytes, 6 桁
"café"          4 chars（分解形なら 5）, 4 桁
"👨‍👩‍👧"     1 grapheme, 複数スカラー, 2 桁
"→"             1 char, 設定により 1 桁または 2 桁
```

必ず `src/width.rs` のヘルパーを使う。`unicode-width` と
`unicode-segmentation` をラップしている。

## ヘルパー

```rust
/// 端末の桁数での表示幅。
pub fn width(s: &str) -> usize;

/// 最大 `max` 桁に切り詰める。切った場合は `…` を付ける。
/// grapheme cluster を分割しない。
pub fn truncate(s: &str, max: usize) -> String;

/// ちょうど `target` 桁になるよう空白で埋める。長すぎる場合は切り詰める。
pub fn pad(s: &str, target: usize) -> String;

/// 各行が最大 `max` 桁になるよう折り返す。
pub fn wrap(s: &str, max: usize) -> Vec<String>;
```

必要な操作が無い場合は、呼び出し側で書かずに `src/width.rs` に追加すること。

## Ambiguous 幅

`※` `→` `±` `α`、罫線素片など East Asian Ambiguous に分類される文字は、
端末とフォントの組み合わせによって 1 桁にも 2 桁にもなる。実行時に判別する
方法は存在しない。

利用者が決める。

```toml
[display]
ambiguous_width = "wide"   # wide | narrow
```

`width()` がこれを設定から読む。呼び出し側で `unicode_width` の
`width()` / `width_cjk()` を直接呼ばないこと。この選択は 1 箇所に閉じる。

既定は `wide`。日本語環境の端末設定の多数派に合わせる。

## char ではなく grapheme で反復する

grapheme cluster の途中で切ると文字化けする。結合文字が別の基底文字に付いたり、
旗の絵文字が半分になったりする。

```rust
// 誤り
for c in s.chars() { ... }

// 正しい
use unicode_segmentation::UnicodeSegmentation;
for g in s.graphemes(true) { ... }
```

切り詰め、カーソル移動、バックスペースの処理で効いてくる。

## ratatui との関係

ratatui は内部で `unicode-width` を使うので、`Paragraph` に日本語を渡せば
それなりに描画される。問題が出るのは、こちらが何かを計算したときである。

- ラベルの長さをもとに `Rect` を手で分割する
- `Table` のカラム幅を決める
- `Block` に渡す前にタイトルを切り詰める
- カーソル位置を計算する

これらはすべて `width()` を経由すること。

## レイアウト作業を終える前の確認

- 差分の中で `.len()` や `.chars().count()` を幅として使っていないか
- 切り詰めはすべて `truncate()` を通っているか
- 固定幅のセルはすべて `pad()` を通っているか
- 文字を切る箇所で char ではなく grapheme を使っているか
- `ambiguous_width` を尊重しているか。ハードコードしていないか

## 書くべきテスト

レイアウトに関わるコードには最低限これらを入れる。

```rust
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
```

日本語の曜日ヘッダ（`日 月 火 水 木 金 土`）を使ったカレンダーグリッドの
テストは持っておく価値がある。ずれが最初に現れるのがここだから。

## レイアウトが崩れたときの調査手順

1. ASCII だけで再現するか確認する。問題なければ幅のバグ。
2. そのモジュールで `.len()` と `.chars().count()` を検索する。
3. 問題の文字が Ambiguous クラスに属するか確認する。該当文字 1 個につき
   ちょうど 1 桁ずれているなら、これが原因。
4. 端末側の設定を確認する。iTerm2 と Terminal.app には「曖昧幅の文字を
   全角として扱う」設定があり、`ambiguous_width` との不一致はコードのバグと
   見分けがつかない。
