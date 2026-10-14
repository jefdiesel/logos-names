#!/usr/bin/env node
// Logos Names MCP server: lets an AI agent look names up as tools.
//
// Speaks the Model Context Protocol over stdio (newline-delimited JSON-RPC) with
// no dependencies. Read-only: it resolves names, it never signs or sends anything.
// Reads the Names test chain unless LEZ_RPC_URL and NAMES_PROGRAM say otherwise:
//
//   LEZ_RPC_URL=http://127.0.0.1:3040 NAMES_PROGRAM=<program id> node mcp/server.js
//
// The gateway publishes it bundled into one file as /mcp.js (scripts/publish-cli.sh).

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

/** The Names test chain, reached through the gateway; the CLI defaults to it too. */
const DEFAULT_RPC = "https://logos-names.wrapit.workers.dev/rpc";
const DEFAULT_PROGRAM = "CsiBzfERqtLz9dYV9dPWzFPMbaUcUagYeCmvvJVmCGcF";
const rpcUrl = process.env.LEZ_RPC_URL || DEFAULT_RPC;
const program = process.env.NAMES_PROGRAM || DEFAULT_PROGRAM;
const VERSION = "0.2.0";
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
      "Names expire: `expires` is when, in milliseconds since the Unix epoch, and `lapsed` is " +
      "true once that has passed and the name is in its 30-day grace period, after which it " +
      "is no longer registered. Do not pay a lapsed name without the user knowing it has lapsed. " +
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
        serverInfo: { name: "logos-names", version: VERSION },
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

const HELP = `logos-names-mcp ${VERSION}: look up Logos names from an AI agent

An MCP server over stdio. Your MCP client starts it; you don't run it yourself.
It is read-only: it never signs, pays or sends anything.

Tools:
  resolve_name    a name's owner, the account it stands for, its private
                  receiving keys and its text entries (chat, inbox, agent...)
  get_page        the page kept under a name, as text
  reverse_lookup  the name an account has chosen to be shown as

Add it to Claude Code:
  claude mcp add --scope user logos-names -- node ${process.argv[1] ?? "mcp.js"}

Other MCP clients:
  {"mcpServers": {"logos-names": {"command": "node", "args": ["/path/to/mcp.js"]}}}

Environment:
  LEZ_RPC_URL     sequencer JSON-RPC endpoint (default ${DEFAULT_RPC})
  NAMES_PROGRAM   registry program ID (default ${DEFAULT_PROGRAM})

Page content is written by the name's owner, so get_page returns it as
untrustedContent: an agent should treat it as data, never as instructions.

Needs Node 20 or later. Guide: https://logos-names.wrapit.workers.dev/`;

const flag = process.argv[2];
if (flag === "--version" || flag === "-V") {
  console.log(VERSION);
  process.exit(0);
}
if (flag === "--help" || flag === "-h" || process.stdin.isTTY) {
  console.log(HELP);
  process.exit(0);
}
if (flag) {
  console.error(`logos-names-mcp: unknown option ${flag} (try --help)`);
  process.exit(2);
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
