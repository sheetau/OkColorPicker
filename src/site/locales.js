export const locales = {
  en: {
    title: "OkColorPicker — OKLCH color picker for Windows",
    features: "Features",
    headline: "Color, as you see it.",
    description:
      "Sample colors from your screen and fine-tune them for your favorite apps. Built in Rust for Windows, OkColorPicker uses OKLCH so you can adjust lightness in visually even steps.",
    download: "Download for Windows",
    tools: "Three tools for color",
    guide: "Read the guide",
    descriptions: [
      "Keep hue and chroma fixed as you change lightness. OKLCH makes the steps between shades look more even. Explore sRGB, Display P3, and Rec.2020.",
      "Start with a color you love. Explore variations, build a palette around it, and save your favorites as swatches for later.",
      "Choose a text color and find backgrounds that meet your contrast target, or start with the background. APCA accounts for how we perceive contrast and suggests colors within your chosen gamut. Copy the results as CSS.",
    ],
    integrations: "Works with eight creative apps",
    connection:
      "Send colors to CLIP STUDIO PAINT, Photoshop, Illustrator, and more.",
    connectionsLink: "See all supported apps",
    engine: "Built with Rust and zgui",
    implementation:
      "When a color changes, zgui tracks and redraws only the UI elements it affects. This experimental Rust framework is designed to keep rendering overhead lower than egui or GPUI.",
    source: "Source code",
    language: "Language",
  },
  ja: {
    title: "OkColorPicker — Windows用OKLCHカラーピッカー",
    features: "機能",
    headline: "調色を視覚のままに。",
    description:
      "画面上の色を取得したり、アプリと連携して色を調整。OkColorPickerはOKLCHにより知覚的に均一な配色と調整を提供する、Rust製のWindows用カラーピッカーです。",
    download: "Windows版をダウンロード",
    tools: "3つのカラーツール",
    guide: "操作ガイド",
    descriptions: [
      "色相と彩度を固定して、明度だけを調整できます。OKLCHにより、見た目の明るさが均等に変わる階調を作れます。sRGB、Display P3、Rec.2020に対応。",
      "色のバリエーションを探したり、好きな色に合わせたパレットを生成できます。気に入った色はスウォッチに保存して、いつでも呼び出せます。",
      "文字色または背景色を基準に、目標のコントラスト比を満たす色を提案します。人間の視覚特性を考慮したAPCAを採用し、選択した色域内で最適な色を導き出します。",
    ],
    integrations: "8つの制作アプリと連携",
    connection:
      "CLIP STUDIO PAINT、Photoshop、Illustratorなどへ、選んだ色をそのまま送れます。",
    connectionsLink: "対応アプリ一覧",
    engine: "Rustとzguiで実装",
    implementation:
      "実験的なRust製UIフレームワークであるzguiを採用。変更のあった要素だけの捕捉と再描画を実現し、eguiやGPUIよりも描画負荷を低く抑えています。",
    source: "ソースコード",
    language: "言語",
  },
};
