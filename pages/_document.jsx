import { Html, Head, Main, NextScript } from "next/document";
import { asset } from "../src/site/config.mjs";

const fonts = [
  ["Martian Grotesk", "MartianGrotesk.ttf", "100 1000", "75% 200%"],
  ["Martian Mono", "MartianMono.ttf", "100 800", "75% 112.5%"],
  // Tune only the Japanese fallback to the optical weight of the Martian faces.
  ["M PLUS 2", "MPLUS2-Variable.ttf", "400", "normal", 450],
  ["M PLUS 2", "MPLUS2-Variable.ttf", "500", "normal", 550],
];

export default function Document(props) {
  const locale = props.__NEXT_DATA__?.props?.pageProps?.locale ?? "en";
  return (
    <Html lang={locale}>
      <Head>
        <link rel="icon" href={asset("favicon.ico")} />
        <meta name="color-scheme" content="dark" />
        <meta name="theme-color" content="#0a0a0c" />
        <style>{fonts.map(([family, file, weight, stretch, opticalWeight]) =>
          `@font-face { font-family: "${family}"; src: url("${asset("fonts/" + file)}") format("truetype"); font-weight: ${weight}; font-stretch: ${stretch}; ${opticalWeight ? `font-variation-settings: "wght" ${opticalWeight};` : ""} font-display: swap; }`
        ).join("\n")}</style>
      </Head>
      <body><Main /><NextScript /></body>
    </Html>
  );
}
