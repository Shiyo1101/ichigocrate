# サンプル集

ichigocrate で動く BASIC プログラムのサンプル。

| ファイル | 内容 |
| --- | --- |
| [`net-oxo.bas`](net-oxo.bas) | 〇×ゲーム (三目並べ) のネット対戦。[ichigocrate-relay](https://github.com/Shiyo1101/ichigocrate-relay) を使う |

## 読み込み方

- **貼り付け**: デスクトップ版はファイルの中身をコピーして Cmd+V / Ctrl+V
- **GitHub から取り込む** (デスクトップ版): `NEW` の後に次を実行する。URL は小文字で打つ

  ```basic
  GET "https://raw.githubusercontent.com/Shiyo1101/ichigocrate/main/samples/net-oxo.bas"
  ```

## net-oxo.bas (〇×ネット対戦)

2 台の ichigocrate で〇×ゲームを対戦する。手は中継サーバー [ichigocrate-relay](https://github.com/Shiyo1101/ichigocrate-relay) を介してやりとりする。自分でデプロイした中継サーバーを使うときは、20 行目の URL を書き換える。

### 遊び方

1. 2 台とも `RUN` し、同じ部屋番号 (1〜9999) を入れる。プレイヤー番号は一方が `1` (先手)、もう一方が `2`
2. 自分の番 (`YOUR TURN`) に、置きたいマスの数字キー (1〜9) を押す
3. 3 つ並べたほうの勝ち

対戦ごとに違う部屋番号を使う (前の対戦の手が残っていることがあるため)。
