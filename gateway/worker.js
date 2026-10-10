// Logos Names gateway: serves `<name>.<any domain>` from the name registry.
//
// Runs unchanged as a Cloudflare Worker (route `*.yourdomain/*`) or locally
// through `serve.js` at `<name>.localhost`. The gateway holds no state: every
// answer is read from the LEZ sequencer, so any copy of it serves the same names.
// It derives each name's key and opens what the name holds on the visitor's
// behalf, so it learns every name asked of it.
//
// Configuration (Worker vars or environment):
//   LEZ_RPC_URL     sequencer JSON-RPC endpoint
//   NAMES_PROGRAM   account ID of the deployed registry program
//   ROOT_HOSTS      optional, comma-separated hosts that are the gateway itself.
//                   On those, and on any workers.dev host, the name is the first
//                   path segment: `<host>/<name>`.
//   FAUCET_URL      optional, the chain's faucet. When set, root hosts also
//                   forward `/faucet` to it and `/rpc` to LEZ_RPC_URL, so clients
//                   reach the chain through the gateway's address.

import { isValidName, normalizeName, resolveName, resolvePage } from "../resolver/names.js";

const RECORD_PATH = "/.well-known/logos-name.json";

// What `/rpc` passes on: reads, and sending transactions. Anything else the
// sequencer offers, such as operator methods, stays private.
const RPC_METHODS = new Set([
  "checkHealth", "getAccount", "getAccountAtBlock", "getAccountBalance", "getAccountsNonces",
  "getAccountSummary", "getAccountView", "getAccountViewAtBlock", "getBlock", "getBlockByHash",
  "getBlockById", "getBlockRange", "getBlocks", "getChannelId", "getEvents", "getFeeState",
  "getLastBlockId", "getLastFinalizedBlockId", "getProgramIds", "getProofsAndRoot", "getSchema",
  "getStatus", "getTransaction", "getTransactionsByAccount", "sendTransaction",
]);
const MAX_RPC_BYTES = 2 * 1024 * 1024;

/** Forwards a JSON-RPC request (or batch) to the sequencer if every method is allowed. */
async function proxyRpc(request, rpcUrl) {
  if (request.method !== "POST") return text("POST a JSON-RPC request here", 405);
  const body = await request.text();
  if (body.length > MAX_RPC_BYTES) return text("request too large", 413);
  let calls;
  try {
    calls = [JSON.parse(body)].flat();
  } catch {
    return json({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "Parse error" } }, 400);
  }
  if (!calls.length || !calls.every((call) => RPC_METHODS.has(call?.method))) {
    return json({ jsonrpc: "2.0", id: null, error: { code: -32601, message: "Method not allowed" } }, 403);
  }
  const response = await fetch(rpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body,
  });
  return new Response(response.body, {
    status: response.status,
    headers: { "Content-Type": "application/json", "Access-Control-Allow-Origin": "*" },
  });
}

/** Forwards a faucet request, telling the faucet who is asking so it can limit them. */
async function proxyFaucet(request, faucetUrl) {
  if (request.method !== "GET" && request.method !== "POST") return text("method not allowed", 405);
  const response = await fetch(faucetUrl, {
    method: request.method,
    headers: {
      "Content-Type": "application/json",
      "X-Client-IP": request.headers.get("CF-Connecting-IP") ?? "unknown",
    },
    body: request.method === "POST" ? (await request.text()).slice(0, 4096) : undefined,
  });
  return new Response(response.body, {
    status: response.status,
    headers: { "Content-Type": "application/json", "Access-Control-Allow-Origin": "*" },
  });
}

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
  // Accept the written form too: `<host>/jef.lez`.
  const bare = normalizeName(first);
  const name = isValidName(bare) ? bare : null;
  return { name, path: "/" + rest.join("/"), sharedOrigin: true };
}

export default {
  async fetch(request, env) {
    const { LEZ_RPC_URL: rpcUrl, NAMES_PROGRAM: program } = env;
    if (!rpcUrl || !program) return text("gateway is missing LEZ_RPC_URL or NAMES_PROGRAM", 500);

    const rootHosts = (env.ROOT_HOSTS ?? "")
      .split(",")
      .map((host) => host.trim().toLowerCase())
      .filter(Boolean);
    const url = new URL(request.url);
    const { name, path, sharedOrigin } = target(url, rootHosts);
    if (sharedOrigin && env.FAUCET_URL) {
      try {
        if (url.pathname === "/rpc") return await proxyRpc(request, rpcUrl);
        if (url.pathname === "/faucet") return await proxyFaucet(request, env.FAUCET_URL);
      } catch (error) {
        return text(`could not reach the chain: ${error.message}`, 502);
      }
    }
    if (request.method !== "GET" && request.method !== "HEAD") return text("method not allowed", 405);
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
