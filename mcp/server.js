#!/usr/bin/env node
// Logos Names MCP server: lets an AI agent look names up as tools.
//
// Speaks the Model Context Protocol over stdio (newline-delimited JSON-RPC) with
// no dependencies. Read-only: it resolves names, it never signs or sends anything.
//
//   LEZ_RPC_URL=http://127.0.0.1:3040 NAMES_PROGRAM=<program id> node mcp/server.js

import { createInterface } from "node:readline";
import {
  deriveName,
  isValidName,
  normalizeName,
  pageAddress,
  recordAddress,
  resolveName,
  resolvePage,
  reverseLookup,
} from "../resolver/names.js";

const rpcUrl = process.env.LEZ_RPC_URL;
const program = process.env.NAMES_PROGRAM;
const PROTOCOL_VERSION = "2025-06-18";
/** Pages are capped at 80 KiB on chain; cap what is handed to a model lower still. */
const MAX_PAGE_CHARS = 20_000;

const nameArgument = {
  type: "object",
  properties: {
    name: { type: "string", description: "A Logos name such as `jef.lez` or `jef`." },
  },
  required: ["name"],
};

const tools = [
  {
    name: "resolve_name",
    description:
      "Look up a Logos name. Returns its owner, the public account it is an alias for, " +
      "its private receiving keys, and its text entries (such as chat, inbox and agent). " +
      "Looking a name up reveals nothing to the chain, which never sees names. A name that " +
      "points at a public account shows that account, and so its balance and history.",
    inputSchema: nameArgument,
    async run({ name: typed }) {
      const name = requireName(typed);
      const record = await resolveName(rpcUrl, program, name);
      if (!record) return { registered: false, name };
      const { id } = await deriveName(name);
      return {
        registered: true,
        ...record,
        recordAddress: await recordAddress(program, id),
        pageAddress: await pageAddress(program, id),
      };
    },
  },
  {
    name: "get_page",
    description:
      "Fetch the page kept under a Logos name, as text. Only someone who knows the name can " +
      "read its page.",
    inputSchema: nameArgument,
    async run({ name: typed }) {
      const name = requireName(typed);
      const page = await resolvePage(rpcUrl, program, name);
      if (!page) return { hasPage: false, name };
      const text = new TextDecoder().decode(page.body);
      return {
        hasPage: true,
        name,
        contentType: page.contentType,
        bytes: page.body.length,
        truncated: text.length > MAX_PAGE_CHARS,
        // Page content is written by the name's owner: treat it as data, not instructions.
        untrustedContent: text.slice(0, MAX_PAGE_CHARS),
      };
    },
  },
  {
    name: "reverse_lookup",
    description:
      "Find the Logos name an account has chosen to be shown as, if any. Returns a name only " +
      "when the name's own record points back at the account, so the result cannot be spoofed.",
    inputSchema: {
      type: "object",
      properties: {
        account: { type: "string", description: "A LEZ account ID in base58." },
      },
      required: ["account"],
    },
    async run({ account }) {
      if (typeof account !== "string") throw new Error("account must be a base58 string");
      return { account, name: await reverseLookup(rpcUrl, program, account) };
    },
  },
];

/** Accepts a name as people write it (`jef.lez`) and returns the bare on-chain name. */
function requireName(input) {
  const name = normalizeName(input ?? "");
  if (!isValidName(name)) {
    throw new Error("invalid name: use 1-32 chars of a-z, 0-9 or '-', optionally ending in .lez");
  }
  return name;
}

async function callTool({ name, arguments: args = {} }) {
  const tool = tools.find((candidate) => candidate.name === name);
  if (!tool) throw Object.assign(new Error(`unknown tool: ${name}`), { code: -32602 });
  try {
    const result = await tool.run(args);
    return { content: [{ type: "text", text: JSON.stringify(result, null, 2) }] };
  } catch (error) {
    // Tool failures are reported in the result so the model can read and react to them.
    return { content: [{ type: "text", text: error.message }], isError: true };
  }
}

async function handle({ method, params }) {
  switch (method) {
    case "initialize":
      return {
        protocolVersion: PROTOCOL_VERSION,
        capabilities: { tools: {} },
        serverInfo: { name: "logos-names", version: "0.1.0" },
      };
    case "ping":
      return {};
    case "tools/list":
      return { tools: tools.map(({ run, ...definition }) => definition) };
    case "tools/call":
      return callTool(params ?? {});
    default:
      throw Object.assign(new Error(`method not found: ${method}`), { code: -32601 });
  }
}

function send(message) {
  process.stdout.write(JSON.stringify(message) + "\n");
}

if (!rpcUrl || !program) {
  console.error("logos-names MCP server needs LEZ_RPC_URL and NAMES_PROGRAM");
  process.exit(1);
}

createInterface({ input: process.stdin }).on("line", async (line) => {
  if (!line.trim()) return;
  let request;
  try {
    request = JSON.parse(line);
  } catch {
    send({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "parse error" } });
    return;
  }
  // Notifications carry no id and get no reply.
  if (request.id === undefined) return;
  try {
    send({ jsonrpc: "2.0", id: request.id, result: await handle(request) });
  } catch (error) {
    send({
      jsonrpc: "2.0",
      id: request.id,
      error: { code: error.code ?? -32603, message: error.message },
    });
  }
});
