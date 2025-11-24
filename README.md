# input_player

input analyzerで解析した入力履歴をvigem-clientを使って再生するRustアプリケーション

## 機能

- **仮想コントローラー**: vigem-clientを使用した仮想Xbox 360コントローラー
- **CSV再生**: input_analyzer形式のCSVファイルを読み込んで60fpsで再生
- **手動入力**: GUIから直接ボタン入力とDpad操作が可能
- **左右反転**: 再生中に左右の入力を反転可能
- **ボタンマッピング**: JSONファイルでボタン配置をカスタマイズ可能

## 必要な環境

- Windows 11
- [ViGEmBusドライバー](https://github.com/ViGEm/ViGEmBus/releases)のインストールが必要

## 開発

```bash
# 依存関係のインストール
npm install

# 開発サーバーの起動
npm run tauri dev

# ビルド
npm run tauri build
```

## CSVフォーマット

input_analyzer形式のCSVファイルに対応しています。

- `duration`: 入力を継続するフレーム数
- `direction`: テンキー方式の8方向入力（5=中立, 8=上, 2=下, 4=左, 6=右, 7=左上, 9=右上, 1=左下, 3=右下）
- その他のカラム: ゲーム固有のボタン名（0=OFF, 1=ON）

サンプル: `sample_input/input_sample_01_input_history.csv`

## ボタンマッピング

`button_mapping.json`でCSVのカラム名と仮想コントローラーのボタンをマッピングできます。
