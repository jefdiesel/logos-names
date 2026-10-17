#!/usr/bin/env node
// Logos Names MCP server: lets an AI agent look names up as tools.
//
// Speaks the Model Context Protocol over stdio (newline-delimited JSON-RPC) with
// no dependencies. Read-only: it resolves names, it never signs or sends anything.
// Reads the public LEZ testnet unless LEZ_RPC_URL and NAMES_PROGRAM say otherwise:
//
//   LEZ_RPC_URL=http://127.0.0.1:3040 NAMES_PROGRAM=<program id> node mcp/server.js
//
// The gateway publishes it bundled into one file as /mcp.js (scripts/publish-cli.sh).

import { createInterface } from "node:readline";
import {
  isValidName,
  normalizeName,
  publicView,
  resolveName,
  resolvePage,
} from "../resolver/names.js";

/** The public LEZ testnet's sequencer, reached directly; the CLI defaults to it too. */
const DEFAULT_RPC = "https://testnet.lez.logos.co";
const DEFAULT_PROGRAM = "38jEfVV1RCyMzu1FGbYcCn5FqxCxKyY58VgzjruAh2gq";
const rpcUrl = process.env.LEZ_RPC_URL || DEFAULT_RPC;
const program = process.env.NAMES_PROGRAM || DEFAULT_PROGRAM;
const VERSION = "0.2.0";
const PROTOCOL_VERSION = "2025-06-18";
/** Pages are capped at 80 KiB on chain; cap what is handed to a model lower still. */
const MAX_PAGE_CHARS = 20_000;

const nameArgument = {
  type: "object",
  properties: {
    name: { type: "string", description: "A Logos name such as `jef` (`@jef`, `lns:jef` and the old `jef.lez` are accepted too)." },
  },
  required: ["name"],
};

const tools = [
  {
    name: "resolve_name",
    description:
      "Look up a Logos name. Returns whether it can be paid privately, its public account " +
      "if the holder chose to publish one, and its text entries (such as chat, inbox and " +
      "agent). By default a name is paid privately with `names pay <name>`, and nobody, the " +
      "payer included, learns the account behind it; a published public account shows its " +
      "balance and history to anyone. " +
      "A name held by a deposit never expires: its `expires` is null. Otherwise names expire: " +
      "`expires` is when, in milliseconds since the Unix epoch, and `lapsed` is " +
      "true once that has passed and the name is in its 30-day grace period, after which it " +
      "is no longer registered. Do not pay a lapsed name without the user knowing it has lapsed. " +
      "Looking a name up reveals nothing to the chain, which never sees names, and who holds " +
      "a name is not public: the chain keeps only a tag of the holder's account.",
    inputSchema: nameArgument,
    async run({ name: typed }) {
      const name = requireName(typed);
      const record = await resolveName(rpcUrl, program, name);
      if (!record) return { registered: false, name };
      return { registered: true, ...publicView(record) };
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
];

/** Accepts a name as people write it (`jef`, `@jef`, `lns:jef`, `jef.lez`) and returns the bare name. */
function requireName(input) {
  const name = normalizeName(input ?? "");
  if (!isValidName(name)) {
    throw new Error("invalid name: use 1-32 chars of a-z, 0-9 or '-', such as jef");
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
  resolve_name    whether a name can be paid privately, its public account if
                  the holder published one, and its text entries (chat, inbox...)
  get_page        the page kept under a name, as text

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
