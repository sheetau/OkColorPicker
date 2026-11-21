import { useRef, useState, type MouseEvent, type ReactNode } from "react";
import { getInstaller, releaseFallback } from "../site/download.mjs";

const messages = {
  en: {
    loading: "Finding the latest release…",
    error: "Couldn’t start the download.",
    releases: "Open GitHub Releases",
    success: "Download started.",
  },
  ja: {
    loading: "最新リリースを確認中…",
    error: "ダウンロードを開始できませんでした。",
    releases: "GitHub Releases を開く",
    success: "ダウンロードを開始しました。",
  },
};

export function DownloadLink({ lang, className, children }: {
  lang: keyof typeof messages;
  className?: string;
  children: ReactNode;
}) {
  const [status, setStatus] = useState<"idle" | "loading" | "success" | "error">("idle");
  const pending = useRef(false);
  const copy = messages[lang];

  async function download(event: MouseEvent<HTMLAnchorElement>) {
    if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    if (pending.current) return;
    pending.current = true;
    setStatus("loading");
    try {
      const link = document.createElement("a");
      link.href = await getInstaller();
      link.download = "";
      document.body.append(link);
      link.click();
      link.remove();
      setStatus("success");
    } catch {
      setStatus("error");
    } finally {
      pending.current = false;
    }
  }

  return <>
    <a className={className} href={releaseFallback} onClick={download} aria-busy={status === "loading"}>{children}</a>
    <span className="download-status" role="status" aria-atomic="true">
      {status !== "idle" && copy[status]}
      {status === "error" && <>{" "}<a href={releaseFallback}>{copy.releases}</a></>}
    </span>
  </>;
}
