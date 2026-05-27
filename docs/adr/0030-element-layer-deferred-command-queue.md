# Element Layer の `element_*` を遅延コマンドキューにする

## Context

Hayate の Element Layer は WIT export として `element_create` / `element_set_text` /
`element_set_style` / `element_append_child` / `element_remove` / `element_set_transform` /
`element_set_scroll_offset` / `element_set_src` などの mutation 関数を上位層に公開している。

これまでの実装はすべて「呼び出し即実行」だった。つまり `element_set_style` を呼ぶと
その場で内部 tree（および HTML Mode では DOM）を mutate し、`element_append_child` を
呼ぶと即座に親子関係を組み替える。`render()` は最終的な layout / GPU 投入だけを担当
していた。

このモデルには二つの問題がある。

1. **フラッシュ境界が不明瞭**：上位層（Hayabusa の Signal/Effect 等）は一連の更新を
   一原子として上層から見えるようにしたいが、即時実行モデルでは「ここまでの mutation
   は完了したが描画はまだ」という中間状態が常に観測可能になる。HTML Mode では特に、
   1 つの Signal 更新が複数の `element_set_*` を発火させたとき、ブラウザの incremental
   reflow が各 mutation ごとに走り、中間レイアウトが一瞬見える可能性がある。

2. **再生・記録・最適化の阻害**：mutation がコマンド列として明示的に表現されていない
   ため、バッチ最適化（同一要素への複数の `set_style` を統合する等）や、デバッグ用の
   コマンド列ダンプ、テストでの mutation シーケンス比較が直接行えない。

## Decision

**`element_*` mutation 系関数は呼ばれた時点ではコマンドを `Vec<Command>` に積むだけと
し、`render()` を唯一のフラッシュ境界とする。**

具体的なルール：

- 対象 mutation：`element_create` / `element_set_style` / `element_set_text` /
  `element_set_src` / `element_set_transform` / `element_set_scroll_offset` /
  `element_append_child` / `element_insert_before` / `element_remove` / `set_root`
  および web adapter 固有の `element_set_font_family` / `element_set_aria_label` /
  `element_set_role` / `element_set_text_content`
- これらは内部キューに `Command` を push するだけで、tree / DOM の状態は変更しない
- `render()` の冒頭でキューを順次フラッシュし、その後に既存の layout / GPU 投入
  （Canvas Mode）または背景色更新（HTML Mode）を実行する
- `element_get_text` / `element_get_text_content` 等の読み取り API は**前回 `render()`
  時点のコミット済み状態**を返す。キューに積まれた pending な変更は読み取れない
- `element_create` だけは ID を同期的に返す必要があるため、内部 SlotMap のスロットだけ
  確保する。HTML Mode では DOM 構築自体は `Create` コマンドのフラッシュ時に行う

WIT インターフェースのシグネチャは**一切変更しない**。`flush()` 等の追加 export も
設けない。挙動の変更は WIT の事後条件（postcondition）レベルでの再定義となる。

## Considered Options

- **即時実行 + 別途 `flush()` export を追加**：上位層に明示的なフラッシュタイミングを
  選ばせる案。`render()` 以外のフラッシュ境界を許してしまうと「いつ描画されるか」が
  上位層実装ごとに分かれ、最下層基盤としての挙動保証が弱まる。却下。
- **`element_set_*` だけ遅延、`element_create` / `element_append_child` は即時**：
  ID 確保や親子関係の整合性のために構造変更だけは即時にする案。しかし `set_style` が
  遅延される一方で `append_child` が即時だと、子追加直後にスタイル適用前の中間レイアウト
  が見える問題は解決しない。却下。
- **遅延コマンドキュー（採用）**：すべての mutation を `render()` まで遅延させる。
  フラッシュ境界が `render()` の一点に統一され、上位層が組み立てる「1 フレーム分の
  mutation 列」が原子的に反映される

## Consequences

- `crates/adapters/web/src/element_renderer.rs`：`HayateElementRenderer` / 
  `HayateElementHtmlRenderer` 両方に `command_queue: Vec<Command>` を追加し、各
  `element_*` メソッドはキューへの push のみを行う。`render()` 冒頭で `flush_commands()`
  を呼ぶ
- HTML Mode の `HtmlNode.dom` は `Option<Element>` に変更：`element_create` 直後から
  `Create` コマンドのフラッシュまでの間は DOM がまだ存在しないため
- `element_get_text` 等の読み取りは「直近 `render()` の後でしか正しい値を返さない」と
  いう契約になる。同一フレーム内で `set_text` 直後に `get_text` を呼ぶと古い値が返る
- `on_pointer_down` 等の入力ハンドラは即時実行のまま。入力イベントはユーザー操作起点で
  あり、Element Layer の mutation セマンティクスとは独立した経路
- ADR-0018（export poll モデル）と整合：`render()` を上位層が駆動する設計は維持。
  キュー化により「render() = フラッシュ + 描画」が一つの責務に統合される
