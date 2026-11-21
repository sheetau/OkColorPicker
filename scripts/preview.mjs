import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { createServer } from "node:http";
import { networkInterfaces } from "node:os";
import path from "node:path";
import { basePath } from "../src/site/config.mjs";

const root = path.resolve("out");
const lan = process.argv.includes("--lan");
const port = Number(process.env.PORT ?? (lan ? 3001 : 3000));
const types = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
  ".txt": "text/plain; charset=utf-8",
  ".svg": "image/svg+xml",
  ".ico": "image/x-icon",
  ".png": "image/png",
  ".webp": "image/webp",
  ".ttf": "font/ttf",
};

createServer(async (request, response) => {
  try {
    const url = new URL(request.url, "http://localhost");
    if (basePath && ["/", basePath].includes(url.pathname)) {
      response.writeHead(302, { Location: `${basePath}/` }).end();
      return;
    }
    if (!url.pathname.startsWith(`${basePath}/`)) throw new Error("Not found");
    const relative = decodeURIComponent(url.pathname.slice(basePath.length));
    let filename = path.resolve(root, `.${relative}`);
    if (filename !== root && !filename.startsWith(`${root}${path.sep}`)) throw new Error("Not found");
    if ((await stat(filename)).isDirectory()) filename = path.join(filename, "index.html");
    const info = await stat(filename);
    response.writeHead(200, {
      "Content-Type": types[path.extname(filename)] ?? "application/octet-stream",
      "Content-Length": info.size,
      "Cache-Control": "no-cache",
    });
    if (request.method === "HEAD") response.end();
    else createReadStream(filename).on("error", () => response.destroy()).pipe(response);
  } catch {
    response.writeHead(404).end("Not found");
  }
}).listen(port, lan ? "0.0.0.0" : "127.0.0.1", () => {
  console.log(`Preview: http://localhost:${port}${basePath}/`);
  if (lan) {
    for (const address of Object.values(networkInterfaces()).flat()) {
      if (address?.family === "IPv4" && !address.internal)
        console.log(`LAN: http://${address.address}:${port}${basePath}/`);
    }
  }
});
