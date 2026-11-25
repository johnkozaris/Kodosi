// Windows App Installer fetches .appinstaller files with HEAD and byte-range GET requests and
// fails without Last-Modified or HTTP 206 responses. Cloudflare Pages provides neither for static
// assets, so this worker serves the file instead.
const MODIFIED = "__MODIFIED__";

export default {
  async fetch(request, env) {
    const asset = await env.ASSETS.fetch(new Request(request.url, { method: "GET" }));
    if (!asset.ok) {
      return asset;
    }
    const body = new Uint8Array(await asset.arrayBuffer());
    const headers = new Headers(asset.headers);
    headers.set("Accept-Ranges", "bytes");
    headers.set("Last-Modified", MODIFIED);
    const size = body.byteLength;
    const range = /^bytes=(\d*)-(\d*)$/.exec(request.headers.get("Range") ?? "");
    let [status, start, end] = [200, 0, size - 1];
    if (range && (range[1] || range[2])) {
      start = range[1] ? Number(range[1]) : Math.max(size - Number(range[2]), 0);
      end = range[1] && range[2] ? Math.min(Number(range[2]), size - 1) : size - 1;
      if (start > end) {
        headers.set("Content-Range", `bytes */${size}`);
        headers.set("Content-Length", "0");
        return new Response(null, { status: 416, headers });
      }
      status = 206;
      headers.set("Content-Range", `bytes ${start}-${end}/${size}`);
    }
    headers.set("Content-Length", String(end - start + 1));
    return new Response(request.method === "HEAD" ? null : body.slice(start, end + 1), { status, headers });
  },
};
