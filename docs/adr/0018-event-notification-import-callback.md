# イベント通知は WIT import callback モデルを採用する

## Context

Hayate Element Layer がイベント（click / focus / text-input / IME 等）を上位層（Hayabusa および他言語 SDK）に通知する方式を決定する必要がある。

候補は二つあった。

1. **import callback モデル**: Hayate が WIT import として上位層のコールバック関数を呼ぶ。Hayate がヒットテストしてイベントを即座に上位層に通知する
2. **export poll モデル**: Hayate がイベントをキューに貯め、上位層が `poll-events()` を呼んで取り出す

Hayate は言語非依存の基盤（ADR-0012）であり、Hayabusa（Rust、別リポジトリ）だけでなく TypeScript・C・Python 等の SDK も上位層として想定される。

## Decision

**import callback モデルを採用する。**

WIT の `import` として上位層のイベントハンドラを呼び出す。全イベント種別（click / focus / scroll / composition-start / composition-update / composition-end / commit-text 等）を単一の `on-event` import に統一する。

```wit
// 上位層が実装し Hayate が import する
import on-event: func(element-id: element-id, event: event);
```

## Considered Options

- **export poll モデル（却下）**: 上位層がフレームごとに `poll-events()` を呼ぶ方式。上位層の実行モデルへの依存がない点は有利だが、フレームレイテンシ（≤16ms）が常に挟まる。Signal ベースのリアクティブフレームワーク（Hayabusa）では「イベント発生 → Signal 即時更新」が自然であり、poll は不要な間接層になる
- **import callback モデル（採用）**: Wasm コンポーネントモデルの標準パターン。Wasm はシングルスレッド（ADR-0003）かつ同期実行モデルであり、import 呼び出しは呼び出し元スタック上で完結する。再入問題は下記の制約で排除できる

## Constraints

`on-event` コールバックの実装は、**コールバック内で Hayate の WIT export を呼び返してはならない**（Element Layer mutation 等）。イベント処理と Element Layer mutation は必ず別フェーズで行うこと。Hayabusa の場合、`on-event` 内では Signal の値更新のみを行い、Element Layer への反映は次のレンダーパスで実施する。

この制約はすべての言語 SDK に共通して課される。

## Consequences

- イベントレイテンシはヒットテスト完了直後（サブフレーム）になる
- Hayabusa および他言語 SDK は WIT import として `on-event` を実装する義務を持つ
- WIT が Hayate と上位層（Hayabusa 等）の物理的な境界線であり、import/export の非対称性がその境界を明示する
