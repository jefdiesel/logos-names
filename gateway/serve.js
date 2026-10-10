// Runs the gateway locally: `<name>.localhost:<port>` resolves to this machine in
// Chrome and Firefox with no DNS setup, and each name gets its own browser origin.
//
//   LEZ_RPC_URL=http://127.0.0.1:3040 NAMES_PROGRAM=<program id> node gateway/serve.js

import { createServer } from "node:http";
import worker from "./worker.js";

const port = Number(process.env.PORT ?? 8787);
const env = {
  LEZ_RPC_URL: process.env.LEZ_RPC_URL,
  NAMES_PROGRAM: process.env.NAMES_PROGRAM,
  ROOT_HOSTS: process.env.ROOT_HOSTS,
  FAUCET_URL: process.env.FAUCET_URL,
};

createServer(async (incoming, outgoing) => {
  const chunks = [];
  for await (const chunk of incoming) chunks.push(chunk);
  const hasBody = incoming.method !== "GET" && incoming.method !== "HEAD";
  const request = new Request(`http://${incoming.headers.host}${incoming.url}`, {
    method: incoming.method,
    headers: incoming.headers,
    body: hasBody ? Buffer.concat(chunks) : undefined,
  });
  const response = await worker.fetch(request, env);
  outgoing.writeHead(response.status, Object.fromEntries(response.headers));
  outgoing.end(Buffer.from(await response.arrayBuffer()));
}).listen(port, "127.0.0.1", () => {
  console.log(`Logos Names gateway on http://<name>.localhost:${port}`);
});
