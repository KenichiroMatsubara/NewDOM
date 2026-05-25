# セッションメモ：CSS機能Tier・HTMLモードdirty tracking・速さの根源

## 1. CSS機能 Tier表

### Tier 1：絶対採用（UIの基盤として必須）

| 機能 | Hayateでの実装形 |
|---|---|
| transform | GPUのmatrix変換。layout再計算なし。アニメーション基盤 |
| z-index | SceneGraphのrender order管理 |
| overflow: scroll | clip + scroll offset管理 |
| スクロールバー | scroll-viewに付随 |
| border-radius描画 | Velloがround rectを直接サポート。部分実装済み |
| 背景描画（background-image等） | Velloで画像テクスチャ描画 |
| SVG描画 | Velloの本領。compute shaderが得意 |
| フォント描画 | Linebender text stack（Parley/Skrifa） |
| テキスト折り返し処理 | Parley |
| 行レイアウト（line layout） | Parley |
| マウスイベント | WITに既出（click, scroll） |
| キーボードイベント | WITに既出 |
| IME入力 | ADR-0017で設計済み |
| アクセシビリティツリー | AccessKit（ADRに既出） |
| フォーカス管理 | WITにfocus/blur既出 |

### Tier 2：採用（有益、実装複雑でも害なし）

| 機能 | Hayateでの実装形 |
|---|---|
| :hover :focus :active | state flag API。CSS構文不要 |
| ::before ::after | synthetic child nodeとして生成 |
| CSS変数 | design token / theme struct として（文字列解釈なし） |
| @media | responsive breakpoints API として（CSS構文なし） |
| @container | component単位のbreakpoints として |
| CSSアニメーション | Hayabusaレベルのkeyframe補間として |
| CSSトランジション | Hayabusaのsignal補間として |
| filter | Vello対応範囲（blur等）に限定 |
| backdrop-filter | 追加renderpassが必要だが害はない |
| box-shadow | Velloでblur+offset描画 |
| text-shadow | 同上 |
| position: sticky | scroll offsetとの連動。複雑だが害なし |

### Tier 3：優先度低い採用（害はないが需要が薄い）

| 機能 | 理由 |
|---|---|
| ルビ | CJK対応。Parleyの将来実装。日本語UIには必要だが今は不要 |
| 縦書き（writing-mode） | CJK対応。複雑だが害なし。将来フェーズ |
| @supports | Hayateでは機能可否はコンパイル時/起動時に確定。runtime APIとしての需要が薄い |
| Canvas描画 | Raw Layerで概念的にカバー済み。独立した機能として追加する必要性が低い |

### Tier 4：有害（採用しない）

| 機能 | 有害な理由 |
|---|---|
| CSSセレクタ | セレクタマッチングO(n×m)のコストがHayateに逆輸入される |
| CSSパーサ | 文字列解釈APIがHayateの型安全な設計を崩す |
| CSSカスケード | Hayateが捨てた最大の重荷。再導入すればstyle解決がO(tree)になる |
| CSS継承 | 親→子への自動伝播が予測不能な再計算を生む |
| HTMLパーサ | Hayateをブラウザにする。スコープ外で利益ゼロ |
| Shadow DOM | SceneGraphと競合するDOM概念。二重のツリー管理が発生 |
| DOMイベントシステム | Hayateのpoll-eventsと競合する第二のイベント系が生まれる |

---

## 2. HTMLモードのdirty tracking

### 現状の問題

`HayateHtmlRenderer`（`crates/adapters/web/src/wasm_impl.rs`）は現在`node_update()`を持たない。
ノードの位置・色・サイズが変わると`node_remove()` → `node_create()`（div破棄→再生成）しかやりようがない。

```rust
// 今あるAPI
node_create()  // div生成してDOMに追加
node_remove()  // divをDOMから削除
render()       // コンテナの背景色だけ更新（DOM nodeには触らない）

// ない
node_update()  // ← これが必要
```

### dirty trackingとは何か

「前フレームから変わったノードのdivプロパティだけ書き換え、変わっていないノードには触らない」こと。

```
dirty tracking なし（現状）:
  変更 → node_remove(id) → node_create(新props) → DOM要素を壊して作り直し

dirty tracking あり:
  変更 → node_update(id, 新props) → el.style.left = "15px" のみ
  （999個の他ノードは何もしない）
```

### なぜHTMLモードで特に重要か

WASM→DOM境界を越えるコストが1回ごとに重い。

```
style.set_property() 1回 ≈ 0.01〜0.05ms

1000ノード全更新（現状）:
  1000回 × 5プロパティ = 5000回crossing ≈ 50〜250ms（1フレーム破綻）

dirty tracking で変更10ノードだけ更新:
  10回 × 変更プロパティ数 ≈ 0.5〜2.5ms（余裕）
```

### 実装はどこを触るか

- **Core（`crates/core/`）：触らない**
  - `HayateHtmlRenderer`はSceneGraphを使っていない。DOMそのものが状態。
- **WIT（`wit/hayate.wit`）：`node-update()`を追加**
- **Adapter（`crates/adapters/web/src/wasm_impl.rs`）：実装追加**

```rust
pub struct HayateHtmlRenderer {
    container: HtmlElement,
    nodes: HashMap<u64, Element>,
    prev_props: HashMap<u64, RectProps>,  // 追加：前フレームの状態
    next_id: u64,
}

pub fn node_update(&mut self, id: f64, x: f32, y: f32, ...) {
    let prev = self.prev_props.get(id);
    let el = self.nodes.get(id);
    if prev.x != x { el.style.set_property("left", ...) }
    if prev.color != color { el.style.set_property("background-color", ...) }
    // 変わった分だけDOM操作
}
```

Hayabusa（Signalフレームワーク）が「何が変わったか」を知っているため、`node_update()`を変更ノードにだけ呼ぶことで二重のdiff計算も不要になる。

---

## 3. Hayateの速さの根源

### WebGPU・Velloは「速さの根源」ではない

ブラウザ（Chrome/Firefox/Safari）も全てGPUレンダリングを行っている。
Velloのcompute shaderによるpath renderingは新しいアプローチだが、単純なUI矩形の描画では大差なし。
WebGPUはブラウザの検証層を経由するため、ブラウザ内部のネイティブGPU APIより**遅い可能性すらある**。

### TaffyのCSS計算速度もブラウザに圧勝はできない

- TaffyはWASM上で動作（ネイティブ比 約1.5〜2倍遅い）
- ブラウザのLayoutNG（Chrome）はC++でネイティブ実行、数十年の最適化済み
- Flex/Grid/Blockに絞っているシンプルさの優位性を、WASM実行オーバーヘッドが相殺する

### 速さの本質：「無駄な計算をそもそもしない」

```
ブラウザが遅い理由の本体:
  CSSセレクタマッチング     O(n × m)
  カスケード・詳細度計算    O(tree depth)
  CSS仕様の副作用処理       float, writing-mode, multi-column...
  JS ↔ Layout間の同期      layout thrashing
  DOMのGCプレッシャー       GC pause

Hayateでこれらが発生しない理由:
  スタイルは直接セット      セレクタマッチングなし
  カスケードなし            スタイル解決がO(1)
  Flex/Grid/Blockのみ       実装が単純、コードパスが少ない
  Retained mode             変更分のmutationのみ
  Rustのメモリモデル        GC pauseなし
  layout thrashingが構造的に起きない
```

### モード別の正直な評価

| シナリオ | React | SolidJS | Hayate HTML | Hayate Canvas |
|---|---|---|---|---|
| 静的UI初回描画 | 基準 | 1.5〜2x | 1.2〜1.8x | 0.8〜1.2x（WASM起動コスト±） |
| 小さい更新 | 基準 | 3x | 2〜4x | 3〜5x |
| アニメーション（60fps） | 基準 | 3x | 1.5〜3x | 5〜10x |
| 1000動的ノード更新 | 基準 | 2.5x | 1.5〜3x | 4〜8x |
| テキスト主体・静的 | 基準 | 1.5x | 1〜1.5x | 0.5〜1.0x（劣後の可能性） |

**HTMLモードとSolidJSの差が小さい理由**：SolidJSもSignal+direct DOMでCSSカスケードを避けているわけではないが、WASMブリッジコストがHayateの優位性を相殺する。`node_update()`実装（dirty tracking）でWASM crossing回数を減らすことがHTMLモードの最重要課題。

**Canvasモードが圧勝できるシナリオ**：動的・アニメーション重視のUI。DOM repaintの連鎖がゼロで、layout thrashingが構造的に存在しないため。テキスト主体・静的コンテンツでは劣後する可能性がある。

### 一言まとめ

> **「30年分のCSS仕様と後方互換性を守りながらレンダリングする」コストを根本から捨てたことが速さの正体。GPUやTaffyの速度は主因ではない。**
