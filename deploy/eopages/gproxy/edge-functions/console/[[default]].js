// EdgeOne Pages serves exact static files from /console. This catch-all handles
// extensionless SPA routes like /console/providers by returning index.html.

export async function onRequest(context) {
  const url = new URL(context.request.url);
  if (url.pathname === "/console") {
    url.pathname = "/console/";
    return Response.redirect(url.toString(), 308);
  }

  const last = url.pathname.split("/").pop() || "";
  if (last.includes(".") && url.pathname !== "/console/index.html") {
    return new Response("not found", {
      status: 404,
      headers: { "content-type": "text/plain; charset=utf-8" },
    });
  }

  const indexUrl = new URL("/console/index.html", context.request.url);
  const response = await fetch(indexUrl);
  const headers = new Headers(response.headers);
  headers.set("cache-control", "no-cache");
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}
