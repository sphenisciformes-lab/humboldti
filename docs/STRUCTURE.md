# ディレクトリ構造

このリポジトリのファイルは、役割によって3つに分かれる。どこに何を置くか迷ったら
この分類に照らして判断する。

| 分類 | 内容 | git | crates.io に含める |
|---|---|---|---|
| 開発コード | Rust のソースとテスト | する | する |
| 配布物 | 利用者が受け取るもの | する | する |
| 開発支援 | Claude Code と開発者のためのもの | する | **しない** |

「git にはコミットするが crates.io には含めない」ものがある点が重要。
crates.io のパッケージは利用者がダウンロードするので、開発時にしか使わない
ファイルを入れると無駄に重くなる。除外は `Cargo.toml` の `exclude` で行う。

## 全体像

```
humboldti/
│
├── Cargo.toml              配布  パッケージ定義。exclude の設定もここ
├── Cargo.lock              開発  バイナリクレートなのでコミットする
├── .gitignore              開発
│
├── README.md               配布  英語。プロジェクトの顔
├── README.ja.md            配布  日本語版（任意）
├── LICENSE-MIT             配布
├── LICENSE-APACHE          配布
│
├── src/                    開発コード
│   ├── main.rs             エントリポイント、ディスパッチ
│   ├── cli.rs              clap のコマンド定義
│   ├── action.rs           Action enum とキー解決
│   ├── width.rs            表示幅ヘルパー（最初に作る）
│   ├── config/
│   │   └── mod.rs          Config 構造体、既定値、読み込み
│   ├── notes/
│   │   └── mod.rs          パス解決、追記、パース
│   └── ui/
│       ├── mod.rs
│       ├── calendar.rs     カレンダー画面
│       └── search.rs       検索画面
│
├── tests/                  開発コード  結合テスト
│
├── docs/                   開発支援
│   ├── STRUCTURE.md        このファイル
│   ├── DESIGN.md           設計判断とその理由
│   └── ROADMAP.md          実装順序とスコープ
│
├── mcpb/                   開発支援  Claude Desktop 拡張機能(.mcpb)のパッケージ元
│   └── manifest.json       手元で `mcpb pack` する際のテンプレート。
│                           `server/`(ビルド済みバイナリ)は生成物なので
│                           .gitignore 済み。手順は README 参照
│
├── CLAUDE.md               開発支援  Claude Code が毎回読む
└── .claude/                開発支援
    └── skills/
        ├── east-asian-width/SKILL.md
        └── add-subcommand/SKILL.md
```

## 各分類の判断基準

### 開発コード

`src/` と `tests/` のみ。ここに置くのは Rust のソースだけで、設定サンプルや
ドキュメントは入れない。

`src/` のモジュール分けは責務で切る。1ファイルが 300 行を超えたら分割を検討する。
`ui/` 配下は画面ごとに 1 ファイル。

### 配布物

利用者が受け取るもの。README とライセンスは crates.io のページに表示されるので、
これらが不完全だと信頼されない。

`README.md` は英語で書く。国際的に公開する以上、これが正本になる。日本語版が
必要なら `README.ja.md` を別に置き、英語版から相互リンクする。

### 開発支援

Claude Code と開発者のためのもの。git にはコミットする（将来コントリビュータが
来たときに共有されるため）が、crates.io には含めない。

`CLAUDE.md` は毎セッション読まれるので短く保つ。理由や背景は `docs/` に書き、
`CLAUDE.md` からは参照するだけにする。ここが膨らむとセッションのたびに文脈を
食い潰す。

`.claude/skills/` には、Claude Code が繰り返し間違えることを防ぐ手順を置く。
一般的な Rust の知識は書かない（それは既に知っている）。このプロジェクト固有の
落とし穴だけを書く。

## 判断に迷ったとき

**新しいドキュメントはどこに置くか。**
利用者が読むものなら `README.md` に統合するか、`docs/` に置いて README から
リンクする（この場合 `exclude` から外す）。開発者だけが読むものなら `docs/`。

**設定ファイルのサンプルはどこに置くか。**
置かない。`pen config init` が既定値を出力するので、サンプルファイルを別に
持つと二重管理になり、必ずズレる。

**スクリーンショットや図はどこに置くか。**
`docs/assets/` を作る。README から参照する場合は `exclude` に注意すること。
crates.io は相対パスの画像を表示できないので、README には GitHub の絶対 URL を
書く。
