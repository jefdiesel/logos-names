# Logos Names

A private name registry for the Logos Execution Zone (LEZ v0.3). A name such as
`jef.lez` resolves to:

- a **wallet**: the keys for a private payment, and/or a public account,
- **text entries**: free-form key/value pairs such as `chat`, `inbox` or `agent`,
- a **page**: up to about 80 KiB served at `jef.<any gateway domain>`,
- optionally a **reverse record**: the name an account publishes for itself.

Knowing a name is the only way to find it. The chain never sees a name: it
holds a 32-byte identifier derived from the name and a profile sealed under a
key that is also derived from the name. There is no list of names and nothing
links one record to another, so nobody can browse the registry. Anyone who
knows or guesses a name can resolve it, which is what lets them pay it.

A name that points only at private receiving keys reveals nothing about its
holder's accounts, and each payment to it lands where only the holder can find
it. What a private payment hides beyond that depends on the sender: sent from a
public account, the sender and the amount are visible; sent from a private
account, neither is.

Every lookup is one derivation and one `getAccount` call against a sequencer.
There is no indexer and no server-side state.

[SPEC.md](SPEC.md) is the specification: everything a wallet, app or gateway
needs to resolve and change names without using this repository's code,
including exactly what is and is not private.

## Status

Working end to end on a local LEZ v0.3.0 chain: deploy, register, update,
transfer, release, resolve, and serve a page by name through the gateway. Not
yet deployed to the public testnet. Written directly against `lee_core` because
the SPEL framework does not support LEZ v0.3 yet (`logos-co/spel#291`).

## Layout

| Path | What it is |
|---|---|
| `core/` | The program (`plan`, `apply`), its on-chain types and address derivation. Unit-tested on the host. |
| `methods/guest/` | The on-chain program: a thin wrapper around `core`. |
| `methods/` | Builds the guest with the local RISC Zero toolchain. |
| `seal/` | What a client adds: deriving a name's identifier and key, and sealing and opening profiles and pages. |
| `cli/` | `names`, the command-line client. Signs and pays through the LEZ wallet. |
| `resolver/names.js` | Dependency-free resolver for Node, browsers and Workers. |
| `gateway/` | Serves `<name>.<domain>`; runs as a Cloudflare Worker or locally. |
| `mcp/server.js` | Read-only MCP server so AI agents can look names up as tools. |
| `basecamp-module/` | `names`, a Logos core module: lookups and changes for every Basecamp app. |
| `basecamp/` | A Basecamp app to find, register, manage, pay and message names. |
| `reference/` | The earlier SPEL (LEZ v0.2.4) draft, kept for when SPEL supports v0.3. |

## How a name is hidden

```
master = PBKDF2-HMAC-SHA256(name, "logos-names/v2", 100000 rounds)
id     = SHA-256("logos-names/v2/id"  ‖ master)     what the chain knows the name by
key    = SHA-256("logos-names/v2/key" ‖ master)     opens and seals what it resolves to
```

The record lives at `PDA(program, SHA-256("logos-names/v2/name/" ‖ id))` and
holds `{version, owner, sealed}`; the page lives at a second derived account.
`sealed` is AES-256-GCM over the profile, which the program stores and cannot
read. Names are 1-32 characters of `a-z`, `0-9` and `-`, written with a `.lez`
suffix; clients strip the suffix and lower the case before deriving.

The slow derivation makes scanning for names expensive. It does not protect a
short or obvious name from someone who guesses it. The holder's account is
public, because it signs every change: the chain shows that an account holds
some identifier, not which name. SPEC.md section 1 has the full account.

## Build and test

Requires Rust 1.98.1 and the RISC Zero toolchain (`rzup install cargo-risczero 3.0.5`,
`rzup install rust`).

```sh
cargo test -p names_core -p names_seal   # program logic and sealing, no chain needed
cargo build --release -p names_methods   # guest -> target/riscv-guest/.../logos_names.bin
cargo build --release -p names_cli       # the `names` client
node --test resolver/names.test.js       # the JavaScript resolver
```

The guest build above is not reproducible. `cargo risczero build --manifest-path
methods/guest/Cargo.toml` produces a binary others can re-derive from source,
but needs Docker with more than 6 GB of memory on Apple silicon. CI builds it.

## Run against a local chain

With a LEZ v0.3.0 standalone sequencer on port 3040 and a wallet home that holds
a funded account:

```sh
export LEE_WALLET_HOME_DIR=/path/to/wallet-home

# Deploy: one header account plus several segment accounts. Pass the wrong
# number and the loader says how many it needs.
wallet program-loader deploy --elf logos_names.bin \
  --header Public/<new account> --segments Public/<new account> ... \
  --payer Public/<funded account>

export LOGOS_NAMES_PROGRAM=<header account id>

names register jef --owner <funded account>
names set-keys jef --npk <hex> --vpk-file vpk.hex   # be paid privately
names set-account jef <account>                      # or publicly: the account is then visible
names set-text jef chat <address>
names set-page jef index.html
names resolve jef
names pay jef 1000 --from <account> --private   # shielded transfer to the name's keys
names pay jef.lez 1000 --from <account>         # to the name's public account
names transfer jef <account>              # new owner; profile and page are emptied
names release jef                         # give it up: nothing of it remains
names set-primary jef --owner <account>   # publish the name on the account, in the clear
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
- `http://jef.localhost:8787/.well-known/logos-name.json` returns the resolved
  name as JSON, for apps and AI agents.

`gateway/` is also a Cloudflare project. To deploy it:

```sh
cd gateway && npm install
cf auth login
NAMES_PROGRAM=<program id> cf deploy
```

A `workers.dev` address has no wildcard subdomains, so it serves names by path
(`<host>/jef`); so does any host listed in `ROOT_HOSTS`. Add a wildcard route
(`*.yourdomain/*`) to serve `jef.yourdomain` instead. Pages served by path are
sandboxed into their own origin, since they would otherwise share one.

A gateway is a convenience, not a trust anchor or a private channel: it learns
every name asked of it and can alter what it serves. The local gateway is the
path that trusts nobody.

## Logos apps

`basecamp-module/` builds `names`, a Logos core module any Basecamp app can
call: `recipient("jef.lez")` returns where to send, privately or publicly, and
`register`, `receive_privately`, `set_text`, `transfer`, `release` and the rest
change a name. The module holds no keys; changes are signed by the LEZ wallet
module (`lez_core`), which must be open. Because the chain keeps no list, the
module remembers on the device which names it has registered or been asked to
keep, and `mine` returns those the open wallet holds. Its interface is in
[SPEC.md](SPEC.md#124-logos-module).

```sh
cd basecamp-module
nix build '.#lgx-portable'                        # result/*.lgx
logosctl package install -y result/*.lgx && logosctl module load names
logosctl call names set_registry str:<program id>
logosctl call names recipient jef.lez
```

Tested in `logosctl` 0.3.1 against a local chain;
`basecamp-module/tools/try-changes.py` runs every change in turn and confirms
each on the chain. `basecamp/` is the Basecamp app (`nix build '.#lgx-portable'`
there too): search for a name, and for the names your wallet holds, a settings
panel for where each is paid, its entries, its page, transferring it and
deleting it. `basecamp/tools/render.py` draws its screens without Basecamp.

## AI agents

`mcp/server.js` is a dependency-free MCP server over stdio with three read-only
tools: `resolve_name`, `get_page` and `reverse_lookup`. It never signs or sends
anything, and it cannot list names: nothing can. To add it to Claude Code:

```sh
claude mcp add logos-names \
  -e LEZ_RPC_URL=http://127.0.0.1:3040 -e NAMES_PROGRAM=<program id> \
  -- node /path/to/logos-names/mcp/server.js
```

Page content is written by a name's owner, so `get_page` returns it in a field
named `untrustedContent`: treat it as data, never as instructions.

## Known limits

- **A guessed name is a known name.** Short and obvious names can be found by
  trying them. The slow derivation raises the cost of trying many; it does not
  hide `jef`.
- **The holder's account is public**, and so is the fact that it holds some
  name. Hold a name from an account used for nothing else if that matters.
- **Your names are remembered by your device, not the chain.** A new device
  does not know which names you hold until you look each one up.
- **A reverse record publishes the name.** It is optional.
- **No fees, expiry or reserved names.** First come, first served, forever.
- **The program is upgradeable** by whoever holds the header account's key.
- **One page per name**, at most about 80 KiB. Larger sites need Logos Storage,
  which a Worker cannot reach without an HTTP bridge.
- **The registry keeps no history, the chain does.** A released name leaves
  nothing in the registry's accounts, and a transferred one keeps nothing of its
  last owner. Past transactions stay in the chain's blocks; what they contain is
  identifiers and sealed bytes.
- **One change per account at a time.** A second change sent before the first
  is in a block is refused; the app and `try-changes.py` wait for each one.
- **A private payment cannot be fully verified by the sender.** The client checks
  that the sender was debited, but only the recipient can confirm it arrived.
- **The Basecamp app pays from public accounts only**, so the sender and amount
  of a payment made from it are visible even when the recipient is not.
