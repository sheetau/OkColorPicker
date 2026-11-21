export const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? "/OkColorPicker";
export const repository = "https://github.com/sheetau/OkColorPicker";
export const siteUrl = `https://sheetau.github.io${basePath}`;
export const releaseApi = "https://api.github.com/repos/sheetau/OkColorPicker/releases/latest";
export const asset = (name) => `${basePath}/media/${name}`;
export const localePath = (lang) => `${basePath}/${lang === "ja" ? "ja/" : ""}`;
