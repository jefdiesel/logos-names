// Logos Names gateway: serves `<name>.<any domain>` from the name registry.
//
// Runs unchanged as a Cloudflare Worker (route `*.yourdomain/*`) or locally
// through `serve.js` at `<name>.localhost`. The gateway holds no state: every
// answer is read from the LEZ sequencer, so any copy of it serves the same names.
//
// Configuration (Worker vars or environment):
//   LEZ_RPC_URL     sequencer JSON-RPC endpoint
//   NAMES_PROGRAM   account ID of the deployed registry program
//   ROOT_HOSTS      optional, comma-separated hosts that are the gateway itself.
//                   On those, and on any workers.dev host, the name is the first
//                   path segment: `<host>/<name>`.

import { isValidName, listNames, resolveName, resolvePage } from "../resolver/names.js";

const RECORD_PATH = "/.well-known/logos-name.json";
const LIST_PATH = "/.well-known/logos-names.json";

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

/**
 * Works out which name a request addresses and the path within that name.
 * On a root host the name is the first path segment; elsewhere it is the subdomain.
 */
function target(url, rootHosts) {
  const host = url.hostname.toLowerCase();
  // workers.dev hosts have no wildcard subdomains, so they are always path-addressed.
  const isRootHost = rootHosts.includes(host) || host.endsWith(".workers.dev");
  if (!isRootHost) {
    return { name: nameFromHost(url.hostname), path: url.pathname, sharedOrigin: false };
  }
  const [, first = "", ...rest] = url.pathname.split("/");
  const name = isValidName(first) ? first : null;
  return { name, path: "/" + rest.join("/"), sharedOrigin: true };
}

export default {
  async fetch(request, env) {
    const { LEZ_RPC_URL: rpcUrl, NAMES_PROGRAM: program } = env;
    if (!rpcUrl || !program) return text("gateway is missing LEZ_RPC_URL or NAMES_PROGRAM", 500);
    if (request.method !== "GET" && request.method !== "HEAD") return text("method not allowed", 405);

    const rootHosts = (env.ROOT_HOSTS ?? "")
      .split(",")
      .map((host) => host.trim().toLowerCase())
      .filter(Boolean);
    const url = new URL(request.url);
    // The list of names is served on any host, since it belongs to the registry, not a name.
    if (url.pathname === LIST_PATH) {
      try {
        const limit = Math.min(Math.max(Number(url.searchParams.get("limit")) || 50, 1), 100);
        const before = url.searchParams.get("before");
        return json(await listNames(rpcUrl, program, { limit, before }));
      } catch (error) {
        return json({ error: error.message }, 502);
      }
    }

    const { name, path, sharedOrigin } = target(url, rootHosts);
    if (!name) {
      const hint = sharedOrigin ? "Add /<name> to this address" : "Put a name in front of this domain";
      return text(`Logos Names gateway. ${hint} to visit a name.`, 404);
    }

    try {
      if (path === RECORD_PATH) {
        const record = await resolveName(rpcUrl, program, name);
        return record ? json(record) : json({ error: "not registered", name }, 404);
      }
      if (path !== "/") return text("not found", 404);

      const page = await resolvePage(rpcUrl, program, name);
      if (page) {
        const headers = {
          "Content-Type": page.contentType,
          "X-Content-Type-Options": "nosniff",
          "Cache-Control": "no-store",
        };
        // Path-addressed names share one origin, so sandbox each page into its own
        // opaque origin; otherwise one name's script could read another's storage.
        if (sharedOrigin) headers["Content-Security-Policy"] = "sandbox allow-scripts allow-forms";
        return new Response(page.body, { headers });
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
