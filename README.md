# ichigocrate

IchigoJam BASIC (子供向け教育用コンピュータ) の C ファームウェアを Rust に書き換え、デスクトップ / Web で動く BASIC インタプリタとして再構築したプロジェクト。

移植元: [github.com/IchigoJam/ichigojam-firm](https://github.com/IchigoJam/ichigojam-firm)

> [!NOTE]
> 「IchigoJam」は[株式会社jig.jp](https://www.jig.jp/)の登録商標です。

コード中の「実機」という単語はIchigoJam ハードウェアを指しています。

## 構成

Cargo ワークスペースとして 3 つのクレートに分かれている。詳細は各ディレクトリの README を参照。

```text
ichigocrate/
├── core/   # no_std 可能な BASIC インタプリタ本体 (ichigocrate-core)   → core/README.md
├── app/    # egui デスクトップフロントエンド (ichigocrate-app)          → app/README.md
└── web/    # WebAssembly フロントエンド (ichigocrate-web)               → web/README.md
```

`core` の描画は `render.rs` (1bpp ビットマップ生成) に集約され、デスクトップ (egui) と Web (canvas 2D) の両フロントエンドが同じラスタライズ結果を共有する。

## クイックスタート

```bash
# デスクトップ
cargo run --release -p ichigocrate-app

# Web (WebAssembly)
cd web && bash ./build.sh && python3 -m http.server   # http://localhost:8000/demo/
```

## ライセンス

MIT ([LICENSE](LICENSE))
