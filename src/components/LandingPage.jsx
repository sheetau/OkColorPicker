import Head from "next/head";
import PickerDemo from "./PickerDemo";
import { DownloadLink } from "./DownloadLink";
import { basePath, localePath, repository, siteUrl } from "../site/config.mjs";
import { locales } from "../site/locales";

function Brand({ locale }) {
  return <a className="brand" href={localePath(locale)} aria-label="OkColorPicker">
    <svg width="24" height="24" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <polygon points="1.07 12.01 8 16 2.14 16 0 13.86 0 2.14 2.14 0 8 0 1.07 4.01 1.07 12.01" />
      <polygon points="16 2.14 16 13.86 13.86 16 8 16 14.93 12.01 14.93 4.01 8 0 13.86 0 16 2.14" />
    </svg>
    <span>OkColorPicker</span>
  </a>;
}

function Header({ locale, copy }) {
  return <header className="masthead wrap">
    <Brand locale={locale} />
    <nav aria-label={locale === "ja" ? "メインナビゲーション" : "Main navigation"}>
      <a className="features-link" href="#features">{copy.features}</a>
      <a href={repository}>GitHub</a>
      <div className="languages" role="group" aria-label={copy.language}>
        {["en", "ja"].map(lang => <a key={lang} lang={lang} hrefLang={lang}
          aria-label={lang === "ja" ? "日本語" : "English"} aria-current={locale === lang ? "page" : undefined}
          href={localePath(lang)}>{lang.toUpperCase()}</a>)}
      </div>
    </nav>
  </header>;
}

function Hero({ locale, copy }) {
  return <section className="hero wrap" aria-labelledby="hero-title">
    <div className="hero-copy">
      <h1 id="hero-title">{copy.headline}</h1>
      <p className="intro">{copy.description}</p>
      <div className="download">
        <DownloadLink lang={locale} className="download-button">
          <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="currentColor">
            <path d="M0 0h7.4v7.4H0zM8.6 0H16v7.4H8.6zM0 8.6h7.4V16H0zM8.6 8.6H16V16H8.6z" />
          </svg>
          {copy.download}
        </DownloadLink>
        <p className="system-requirement">Windows x64 <span>·</span> DirectX 12</p>
      </div>
    </div>
    <div className="demo-window"><PickerDemo lang={locale} /></div>
  </section>;
}

const tools = [
  { name: "Picker", icon: "picker" },
  { name: "Harmony", icon: "harmonies" },
  { name: "APCA", icon: "apca" },
];

function Features({ copy }) {
  return <section id="features" className="features" aria-labelledby="features-title">
    <div className="wrap">
      <div className="section-header">
        <h2 id="features-title">{copy.tools}</h2>
        <a href={repository + "/blob/main/docs/TOOLS.md"}>{copy.guide}</a>
      </div>
      <div className="tool-grid">
        {tools.map(({name, icon}, index) => <article key={name} className="tool">
          <span className="tool-icon"><img src={basePath + "/picker/" + icon + ".svg"} alt="" width="18" height="18" /></span>
          <h3>{name}</h3>
          <p>{copy.descriptions[index]}</p>
        </article>)}
      </div>
      <div className="details">
        <article><h3>{copy.integrations}</h3><p>{copy.connection}<br /><a href={repository + "#connect-another-app"}>{copy.connectionsLink}</a></p></article>
        <article><h3>{copy.engine}</h3><p>{copy.implementation}</p></article>
      </div>
    </div>
  </section>;
}

export default function LandingPage({ locale }) {
  const copy = locales[locale];
  const url = (lang) => siteUrl + (lang === "ja" ? "/ja/" : "/");
  return <>
    <Head>
      <title>{copy.title}</title>
      <meta name="description" content={copy.description} />
      <meta property="og:title" content={copy.title} />
      <meta property="og:description" content={copy.description} />
      <meta property="og:url" content={url(locale)} />
      <link rel="canonical" href={url(locale)} />
      {["en", "ja", "x-default"].map(lang => <link key={lang} rel="alternate" hrefLang={lang} href={url(lang)} />)}
    </Head>
    <Header locale={locale} copy={copy} />
    <main><Hero locale={locale} copy={copy} /><Features copy={copy} /></main>
    <footer className="footer wrap">
      <p lang="en">© 2026 OkColorPicker · Built by <a href="https://github.com/sheetau">sheeta</a></p>
      <div><a href={repository + "/blob/main/LICENSE-APACHE"}>Apache 2.0</a><a href={repository}>{copy.source}</a></div>
    </footer>
  </>;
}
