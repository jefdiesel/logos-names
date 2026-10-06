// Logos Names gateway: serves `<name>.<any domain>` from the name registry.
//
// Runs unchanged as a Cloudflare Worker (route `*.yourdomain/*`) or locally
// through `serve.js` at `<name>.localhost`. The gateway holds no state: every
// answer is read from the LEZ sequencer, so any copy of it serves the same names.
//
// Configuration (Worker vars or environment):
//   LEZ_RPC_URL     sequencer JSON-RPC endpoint
//   NAMES_PROGRAM   account ID of the deployed registry program

import { isValidName, resolveName, resolvePage } from "../resolver/names.js";

const RECORD_PATH = "/.well-known/logos-name.json";

function json(body, status = 200) {
  return new Response(JSON.stringify(body, null, 2) + "\n", {
    status,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "Access-Control-Allow-Origin": "*",
      "Cache-Control": "no-store",
    },
  });
}

function text(body, status) {
  return new Response(body + "\n", {
    status,
    headers: { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "no-store" },
  });
}

/** The name a request is for: the left-most label of a host with at least two labels. */
function nameFromHost(hostname) {
  const isIpAddress = /^[0-9.]+$/.test(hostname) || hostname.includes(":");
  if (isIpAddress) return null;
  const labels = hostname.toLowerCase().split(".");
  return labels.length >= 2 && isValidName(labels[0]) ? labels[0] : null;
}

export default {
  async fetch(request, env) {
    const { LEZ_RPC_URL: rpcUrl, NAMES_PROGRAM: program } = env;
    if (!rpcUrl || !program) return text("gateway is missing LEZ_RPC_URL or NAMES_PROGRAM", 500);
    if (request.method !== "GET" && request.method !== "HEAD") return text("method not allowed", 405);

    const url = new URL(request.url);
    const name = nameFromHost(url.hostname);
    if (!name) return text("Logos Names gateway. Put a name in front of this domain to visit it.", 404);

    try {
      if (url.pathname === RECORD_PATH) {
        const record = await resolveName(rpcUrl, program, name);
        return record ? json(record) : json({ error: "not registered", name }, 404);
      }
      if (url.pathname !== "/") return text("not found", 404);

      const page = await resolvePage(rpcUrl, program, name);
      if (page) {
        return new Response(page.body, {
          headers: {
            "Content-Type": page.contentType,
            "X-Content-Type-Options": "nosniff",
            "Cache-Control": "no-store",
          },
        });
      }
      const record = await resolveName(rpcUrl, program, name);
      return record
        ? text(`${name} is registered but has no page. Its record is at ${RECORD_PATH}`, 404)
        : text(`${name} is not registered.`, 404);
    } catch (error) {
      return text(`could not read the registry: ${error.message}`, 502);
    }
  },
};
