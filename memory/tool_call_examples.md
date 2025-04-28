Example for passing image to another model for generating pyplot code:

```
{
  "action": "analyze_image_and_generate_code",
  "model": "o4-mini",
  "image_url": "<IMAGE_URL>",
  "query": "これはグラフ化可能なデータを含む画像です。画像から数値データを抽出し、Pythonのmatplotlib.pyplotを使用して折れ線グラフを描画する完全なコードを生成してください。"  
}
```
利用例: browserツールでIntelサイトをスクレイピングし、記事をweb_deploy_toolで公開
ツール: browser, web_deploy_tool, memory_tool