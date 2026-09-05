# Humboldti Note

Humboldti Note はターミナルで動く、ミニマルな日次マークダウンメモツールです。

**状態: 日常利用に足る機能は揃っています。crates.io にはまだ未公開です。**

English README: [README.md](README.md)

## 機能

- **記録** : `pen <テキスト>` は今日のファイルに書き込みます。
- **チェックリストの省略記法** :  `pen -t <テキスト>` で `- [ ] <テキスト>` として書き込みます。
- **前日以前のチェックリスト繰り越し** : 未完了の`- [ ]` は今日のファイルを作成すると自動で繰り越されます。
- **カレンダー機能** : `pen cal` は月ごとのグリッドに、書いた分量の濃淡で表示します。
- **全文検索** : コマンドラインからでも、カレンダー画面の中からでも検索できます。
- **日本語対応** : East Asian Width を正しく扱います。
- **キー割り当て変更** : `pen cal` 操作の全キーを `config.toml` で上書きできます。
- **エージェント対応** : 全コマンドの `--json`、チャットに貼り付けるためのトークン予算付き `context`、接続を維持するAIエージェント向けのMCPサーバーを備えています。
- **プレーンファイル** : `~/notes` 以下に1日1ファイルのマークダウンを置きます。データベースも独自形式もありません。`grep`・`git`・クラウド同期がそのまま使えます。

## できること

Humboldti Note には、サブコマンドを伴わない**素の形**と、**サブコマンド**が5つあります。

**素の形**

| 実行 | 内容 |
|---|---|
| `pen <テキスト>` | テキストを今日のファイルへ追記する |
| `pen -t <テキスト>` | 同上。チェックリスト形式(`- [ ] <テキスト>`)で書き込む |
| `pen -` | 追記するテキストを標準入力から読む |
| `pen` | 今日のファイルを `$EDITOR` で開く |

`<テキスト>` はタイプした内容そのまま収録します — 例: `pen 牛乳を買った`。  

`merge_window_minutes`(既定30分)以内の連続した追記は、新しい見出しを作らず同じ時刻見出しにまとまります。  

その日初めての追記・`open`・`pen cal` での訪問のときは、直近の前回のノートに残っている未完了 `- [ ]` を、  
`<!-- carried over from YYYY-MM-DD -->` のコメントを添えて繰り越します。  
実際にファイル上どう見えるかは [ノートの形式](#ノートの形式) を参照してください。

**サブコマンド**

| 実行 | 内容 |
|---|---|
| `pen open [<日付>]` | 指定した日のノートを `$EDITOR` で開きます。`<日付>` は `YYYY-MM-DD`、省略すると今日 |
| `pen cal` | カレンダーを表示します |
| `pen search <クエリ>` | 全文検索(大文字小文字を区別しない正規表現です) |
| `pen context [--since 7d]` | 直近のメモを LLM に渡しやすい形で出力します |
| `pen config <path\|init\|check>` | 設定ファイルの確認・作成 |
| `pen mcp` | MCP 経由で AI エージェントにメモを公開します |

`cal`(対話的なTUI)と `mcp`(値を1つ出力して終わる形ではないMCPサーバー)を除く全コマンドが、  
スクリプトやエージェントから使うための `--json` に対応しています。  
それぞれの詳細は後述の  
[カレンダーと検索](#カレンダーと検索)、[設定](#設定)  
[スクリプトやLLMへメモを渡す](#スクリプトやllmへメモを渡す)  
[AIエージェントから使う](#aiエージェントから使うmcp)  
を参照してください。

## カレンダー

`pen cal` は全画面のカレンダーを開きます。

- 日曜始まりの月グリッドです。
- その日書いた分量(無し/やや多い/かなり多い)を背景色で表します。  
- 選択中の日のノートをプレビューペインに表示します。
- 既定のキー :  
`hjkl`/矢印で日/週移動  
`[`/`]` で月移動、`{`/`}` で年移動  
`Enter` で `$EDITOR` で開く  
`/` で検索開始、`q`/`Esc` で終了  
すべて上書き可能です — [設定](#設定) 参照

## 検索

カレンダーから `/` を押すか、直接 `pen search <検索したい文字列>` を実行すると検索できます。  

- 既定のキー :  
`j`/`k` で結果を移動  
`Enter` で選択中の日を開く  
`q`/`Esc` で戻る  
`会議` のような単語をそのまま書けば、正規表現としてその語を含む行にそのままマッチします。

## ノートの形式

ノートは `~/notes` 以下のような、1日1ファイルのプレーン `.md` ファイルです。

```
~/notes/
  2026/09/2026-09-05.md
  attachments/            # 画像。相対パスで参照
```

データベースも独自形式もロックインもありません。 `grep`・`cat`・`find` がそのまま使えます。  
`git` やお好みのクラウド同期を `~/notes` に向けるだけで動きます。  

繰り越されたチェックリスト項目はファイル上でこう見えます:

```markdown
<!-- carried over from 2026-09-02 -->
- [ ] 未完了のままの何か

## 21:07
今日実際に書いた内容
```

## なぜ

ターミナル向けメモツールは既にありますが、軽量で単純な日次メモ・カレンダー・簡単な記録を兼ね備えたものが必要でした。  
また、日本語の表示幅や IME の挙動を正しく扱うものも必要でした。  
Humboldti Note はその両方のために作られています。

## インストール

**まだリリースタグを打っていないため、現時点で動くのは下記「ソースから」だけです。**  
それ以外は最初のリリースが出た後の手順を先行して書いています。

**Homebrew**(macOS・Linux)

```sh
brew install sphenisciformes-lab/humboldti/humboldti-note
```

**インストールスクリプト**(macOS・Linux ※使用する端末にRust は不要です。)

```sh
curl -fsSL https://github.com/sphenisciformes-lab/humboldti/releases/latest/download/humboldti-note-installer.sh | sh
```

**cargo**(Rust ツールチェーンがあれば任意の環境でインストール可能です。)

```sh
cargo install humboldti-note
```

**ソースから**

```sh
git clone https://github.com/sphenisciformes-lab/humboldti.git
cd humboldti
make install   # または: cargo install --path .
```

いずれの方法でも `pen` コマンドが PATH に入ります。

## 設定

Humboldti Note は何も設定しなくても動きます。  
ノートの保存場所を変えたい、`pen cal` のキー割り当てを変えたい、という場合は次を実行してください。

```sh
pen config init
```

`~/.config/pen/config.toml`(`$XDG_CONFIG_HOME` が設定されていればそちら)に、  
コメント付きの設定ファイルを生成します。値はすべて最初から組み込みの既定値になっているので、  
変えたい項目だけ書き換えて、残りは削除してかまいません(消したキーは既定値にフォールバックします)。  
`pen config path` は何も作らずに解決されたパスだけを表示し、  
`pen config check` は CLI フラグ・環境変数・設定ファイルをすべて適用した後の、実際に効いている値を表示します。

優先順位: `--dir` フラグ > `PEN_*` 環境変数 > 設定ファイル > 組み込みの既定値

| キー | 内容 |
|---|---|
| `notes_dir` | ノートの保存先 |
| `merge_window_minutes` | 何分経つと新しい追記が新しい時刻見出しを作るか |
| `editor` | ノートを開くコマンド。未設定なら `$EDITOR`、それも無ければ `vi` |
| `[keys.calendar]` | カレンダー画面のキー割り当て |
| `[keys.search_input]` | 検索クエリ入力中のキー割り当て |
| `[keys.search_results]` | 検索結果一覧でのキー割り当て |

`editor` に GUI エディタを指定する場合は、ウィンドウを閉じるまで待つフラグ(`code --wait`、`subl --wait` など)を  
必ず付けてください。付けないと、起動コマンドが返った時点(ウィンドウを閉じる前)で `pen` が編集完了と見なしてしまいます。

- キー仕様の書き方:  
`"h"`、`"ctrl-a"`、`"enter"` は `config init` で生成されるファイルのコメントを参照してください。  
ファイル内の知らないキーは警告だけで起動は止まりませんが、パースできないキー仕様や、  
同じテーブル内で2つのアクションが同じキーを取り合っている場合は `pen cal` の起動に失敗します。

## スクリプトやLLMへメモを渡す

メモはプレーンなマークダウンなので、これらが無くても `grep`/`cat`/`find` は既に使えます。  
その上に3つの連携経路がありますが、それぞれ相手側に求める能力が違うだけで、互いに冗長ではありません:

| | 想定する相手 | 出力 | 生存期間 |
|---|---|---|---|
| `--json` | スクリプトや cron | 構造化JSON | 1回実行して終了 |
| `pen context` | チャットへの貼り付け | トークン予算付きのテキスト | 1回実行して終了 |
| `pen mcp` | 常駐するAIエージェント | 都度呼び出せるツール | 常駐し続ける |

シェルを叩いてテキストを解釈することしかできない相手には `--json`。  
まとまったテキストのブロックしか渡せない相手(チャット画面、LLM APIを1回だけ呼ぶスクリプト)には `context`。  
MCP接続を維持してツールを都度呼び出せる相手には `mcp` を使います  
— 詳細は後述の[AIエージェントから使う](#aiエージェントから使うmcp)を参照してください。

### `--json`

`--json` はサブコマンドの前でも後ろでも動きます:

```sh
$ pen --json search 会議
{"hits":[{"path":"/Users/you/notes/2026/09/2026-09-01.md","date":"2026-09-01","line_number":2,"line":"チームMTGで来週のリリースについて会議した"}]}

$ pen --json config check
{"notes_dir":"/Users/you/notes","merge_window_minutes":30,"editor":""}
```

### `pen context`

今日から遡り、トークン予算(`--max-tokens`、既定4000、文字数ベースの概算)を超える手前まで日単位でまとめます。  
ファイルの途中では絶対に切らず、直近1日だけで予算超過でもその1日は必ず返します。

```sh
$ pen context --since 7d
# 2026-09-01
## 10:00
チームMTGで来週のリリースについて会議した

<!-- estimated tokens: 16 / budget: 4000 -->

$ pen context --since 7d | pbcopy    # チャットに貼り付け
$ pen --json context --since 2w > fortnight.json    # スクリプトに渡す
```

## AIエージェントから使う(MCP)

`pen mcp` は [Model Context Protocol](https://modelcontextprotocol.io) のサーバーを stdio 上で動かします。  
公開する3つのツールは、コマンドラインの`pen` と同じコードをそのまま呼んでいます:

- `search_notes(query)` — 大文字小文字を区別しない正規表現でメモ全体を検索
- `read_note(date)` — 指定した日(`YYYY-MM-DD`)のメモを読む
- `append_note(text)` — 今日のメモに追記(`pen <text>` と同じ)

**データの流れ。** Humboldti Note 自身は stdio のみで動作し、メモをどこかへ自発的に送信することはありません。  
ただし接続先次第では送信されます。クラウド型の AI クライアントをつなぐと、  
それらのツールが読んだ内容はあなたのマシンの外に出ます。  
メモに接続する前に、使う MCP クライアントの送信先を確認してください。

`pen mcp` を子プロセスとして実際に起動する何かが必要です — 自分で設定するか、  
(下記の Claude Desktop のように)クライアント側が代わりにやってくれるかのどちらかです。

### Claude Code

```sh
claude mcp add pen -- pen mcp
```

### その他の MCP クライアント(手動設定)

クライアントの MCP 設定に `pen` バイナリを直接指定します:

```json
{
  "mcpServers": {
    "pen": {
      "command": "pen",
      "args": ["mcp"]
    }
  }
}
```

### Claude Desktop(ワンクリックの拡張機能)

Claude Desktop は、ローカルのMCPサーバーを手編集の設定ファイルではなく`.mcpb` 拡張機能としてインストールします。  
手元でパッケージを作る作業が必要です。  
※Node.js が必要なのは `mcpb` パッケージングCLIのためだけで、Humboldti Note本体には不要です:

```sh
cargo build --release
mkdir -p mcpb/server
cp target/release/pen mcpb/server/pen
npx --yes @anthropic-ai/mcpb pack mcpb humboldti-note.mcpb
```

Claude Desktop 側では: 設定 → 拡張機能 → 詳細設定 → 拡張機能開発者 →「拡張機能をインストール...」で、  
今作った `humboldti-note.mcpb` を選択します。  
`mcpb/manifest.json` が「`pen mcp` を実行する」ことをClaude Desktopに伝えているので、  
コマンドを自分で打つ必要はありません。

## ライセンス

以下のいずれかを選択できます。

- Apache License, Version 2.0([LICENSE-APACHE](LICENSE-APACHE) または <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license([LICENSE-MIT](LICENSE-MIT) または <http://opensource.org/licenses/MIT>)
