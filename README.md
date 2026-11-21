# OkColorPicker website

[OkColorPicker](https://github.com/sheetau/OkColorPicker) の日本語・英語ランディングページです。Next.js の Pages Router で各言語の HTML を静的に生成します。

## 開発

Node.js 22 以降で次のコマンドを実行します。

```sh
npm ci
npm run dev
```

英語版は `http://localhost:3000/OkColorPicker/`、日本語版は `http://localhost:3000/OkColorPicker/ja/` で開けます。編集内容は自動で反映されます。

## 検証とビルド

```sh
npm run check
npm run build
npm run preview
```

静的ファイルの出力先は `out/` です。開発用キャッシュには `.next-dev/`、本番用には `.next/` を使用します。プレビューのポートは `PORT` で変更できます。同じ LAN 上の端末から確認する場合は `npm run preview:lan` を実行します。

## GitHub Pages

LP の履歴はアプリと同じ `sheetau/OkColorPicker` リポジトリの `gh-pages` ブランチで管理します。リポジトリの **Settings → Pages → Build and deployment → Source** を **GitHub Actions** に設定すると、`gh-pages` へのプッシュ時にワークフローがテスト・静的ビルド・Pages へのデプロイを行います。公開先は `https://sheetau.github.io/OkColorPicker/` です。

インストーラーのリンクはクリック時に GitHub Releases API から最新版の Windows x64 セットアップを取得します。API のエラー時にはリリースページへのリンクを表示します。
