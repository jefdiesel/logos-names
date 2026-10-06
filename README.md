# Logos Names

A name registry for the Logos Execution Zone (LEZ v0.3). A name such as `jef`
resolves to:

- a **wallet**: a public account, and/or the keys needed for a private transfer,
- **text entries**: free-form key/value pairs such as `chat`, `inbox` or `agent`,
- a **page**: up to 80 KiB served at `jef.<any gateway domain>`,
- a **reverse record**: the name an account wants to be shown as.

Every lookup is one or two `getAccount` calls against a sequencer. There is no
indexer and no server-side state: the accounts that hold a name are derived
from the name itself, so any app, gateway or agent can compute them offline.

## Status

Working end to end on a local LEZ v0.3.0 chain: deploy, register, update,
resolve, and serve a page by name through the gateway. Not yet deployed to the
public testnet. Written directly against `lee_core` because the SPEL framework
does not support LEZ v0.3 yet (`logos-co/spel#291`).

## Layout

| Path | What it is |
|---|---|
| `core/` | All program logic (`plan`, `apply`), record types and address derivation. Unit-tested on the host. |
| `methods/guest/` | The on-chain program: a thin wrapper around `core`. |
| `methods/` | Builds the guest with the local RISC Zero toolchain. |
| `cli/` | `names`, the command-line client. Signs and pays through the LEZ wallet. |
| `resolver/names.js` | Dependency-free resolver for Node, browsers and Workers. |
| `gateway/` | Serves `<name>.<domain>`; runs as a Cloudflare Worker or locally. |
| `mcp/server.js` | Read-only MCP server so AI agents can look names up as tools. |
| `reference/` | The earlier SPEL (LEZ v0.2.4) draft, kept for when SPEL supports v0.3. |

## Where a name lives

All data sits in this program's shard on three kinds of account:

| Data | Account |
|---|---|
| Name record | `PDA(program, SHA-256("logos-names/v1/name/" + name))` |
| Page | `PDA(program, SHA-256("logos-names/v1/page/" + name))` |
| Reverse record | the user's own account |

`PDA(program, seed)` is LEZ's `AccountId::for_public_pda`:
`SHA-256(pad32("/LEE/v0.2/AccountId/PDA/") || program || seed)`.

Names are 1-32 characters of `a-z`, `0-9` and `-`, not starting or ending with `-`.
Records are borsh-encoded; see `core/src/lib.rs` for the layouts and
`resolver/names.js` for a decoder.

A reverse record is only a claim. Resolvers must confirm that the forward
record's `owner` or `account` points back at the same account, as `names reverse`
and `reverseLookup` do.

## Text entry conventions

Text entries are free-form. These keys have a suggested meaning:

| Key | Value |
|---|---|
| `chat` | The owner's Logos Chat address: the 64 lowercase hex characters returned by `chat_module.get_address()`, which a peer passes to `create_conversation`. In Chat 0.3.0 this address changes every time the app starts, so the entry goes stale until the owner updates it. |
| `inbox` | A Logos Delivery content topic, `/{app}/{version}/{name}/{encoding}`, that the owner subscribes to. A topic only routes: anyone can read or publish to it, so confidentiality needs a key published in another entry. Logos defines no per-user inbox convention yet. |
| `agent` | A URL or address where the owner's AI agent can be reached. |
| `cid` | A Logos Storage content ID the name pins. |

## Build and test

Requires Rust 1.98.1 and the RISC Zero toolchain (`rzup install cargo-risczero 3.0.5`,
`rzup install rust`).

```sh
cargo test -p names_core                 # program logic, no chain needed
cargo build --release -p names_methods   # guest -> target/riscv-guest/.../logos_names.bin
cargo build --release -p names_cli       # the `names` client
```

The guest build above is not reproducible. `cargo risczero build --manifest-path
methods/guest/Cargo.toml` produces a binary others can re-derive from source,
but needs Docker with more than 6 GB of memory on Apple silicon.

## Run against a local chain

With a LEZ v0.3.0 standalone sequencer on port 3040 and a wallet home that holds
a funded account:

```sh
export LEE_WALLET_HOME_DIR=/path/to/wallet-home

# Deploy: one header account plus one segment account per 96 KiB of guest binary.
wallet program-loader deploy --elf logos_names.bin \
  --header Public/<new account> --segments Public/<new account> ... \
  --payer Public/<funded account>

export LOGOS_NAMES_PROGRAM=<header account id>

names register jef --owner <funded account>
names set-account jef <account>           # wallet alias
names set-text jef chat <address>
names set-page jef index.html
names set-primary jef --owner <account>   # reverse record
names resolve jef
names pay jef 1000 --from <account>       # send native tokens to the name's account
names set-keys jef --npk <hex> --vpk-file vpk.hex
names pay jef 1000 --from <account> --private   # shielded transfer to the name's keys
```

A private payment generates a real proof (about four minutes on an M4 Mac mini)
and lands on a fresh private account under the recipient's key, which they
discover with `wallet account sync-private`.

Before sending, `names` runs the program's own logic against current chain
state, so a transaction the program would refuse fails locally with the
program's reason and costs no fee. After sending it reads the touched accounts
back: LEZ includes failed transactions in blocks, so inclusion alone does not
mean the change was applied.

## Gateway

```sh
LEZ_RPC_URL=http://127.0.0.1:3040 NAMES_PROGRAM=<program id> node gateway/serve.js
```

- `http://jef.localhost:8787/` serves the page.
- `http://jef.localhost:8787/.well-known/logos-name.json` returns the record as
  JSON, for apps and AI agents.

`gateway/` is also a Cloudflare project. To deploy it:

```sh
cd gateway && npm install
cf auth login
NAMES_PROGRAM=<program id> cf deploy
```

A `workers.dev` address has no wildcard subdomains, so it serves names by path
(`<host>/jef`); so does any host listed in `ROOT_HOSTS`. Add a
wildcard route (`*.yourdomain/*`) to serve `jef.yourdomain` instead. Pages served
by path are sandboxed into their own origin, since they would otherwise share one. A gateway is a convenience, not a trust anchor: it can alter
what it serves, and the local gateway is the path that trusts nobody.

## AI agents

`mcp/server.js` is a dependency-free MCP server over stdio with three read-only
tools: `resolve_name`, `get_page` and `reverse_lookup`. It never signs or sends
anything. To add it to Claude Code:

```sh
claude mcp add logos-names \
  -e LEZ_RPC_URL=http://127.0.0.1:3040 -e NAMES_PROGRAM=<program id> \
  -- node /path/to/logos-names/mcp/server.js
```

Page content is written by a name's owner, so `get_page` returns it in a field
named `untrustedContent`: treat it as data, never as instructions.

## Known limits

- **No fees, expiry or reserved names.** First come, first served, forever.
- **The program is upgradeable** by whoever holds the header account's key.
- **One page per name**, at most 80 KiB. Larger sites need Logos Storage, which
  a Worker cannot reach without an HTTP bridge.
- **Listing names needs an indexer.** The program emits `logos_names::Registered`
  and `logos_names::Transferred` events, but they are emitted before the record
  is checked, so they are hints: confirm each name by reading its record.
  Reading events back has not been tested; the sequencer RPC does not serve them.
- **A private payment cannot be fully verified by the sender.** The client checks
  that the sender was debited, but only the recipient can confirm it arrived.
- **What publishing private receiving keys reveals has not been analysed.**
