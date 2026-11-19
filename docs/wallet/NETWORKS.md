# Names Wallet: networks and the network file

Since wallet-1121 the network is chosen in one place: **Settings → Network**. It offers:

| Choice | What it is | Where its ids come from |
|---|---|---|
| **LEZ 0.4 · Bridge test network** (the default) | Our own LEZ 0.4 chain, `https://lez.nameswallet.app/`, chain id `2da7f7dfb6e51a28c942ea80391aa518253ec515e98d28cb4d83cc9e16ff5374` | Built into the module (`LEZ04_*` in `basecamp-module/rust-lib/src/lib.rs`, `bridge_net.rs`) |
| **LEZ 0.3 · Logos testnet** | The public Logos testnet | Built in (`DEFAULT_SEQUENCER`, `DEFAULT_REGISTRY`) |
| **Custom network** | Any LEZ 0.4 chain, by RPC URL + chain id: one that isn't built in, or ours after a reset | Its **network file** (below) |

Every install, new or updated, opens on our LEZ 0.4 (Jef, 2026-11-19: "theres like zero installs so
next update goes to 0.4"). A `config.json` saved by an earlier build has no `"default_network"` mark:
it opens on LEZ 0.4 once, and its LEZ 0.3 settings are kept as they were. From then on the network
saved is the person's choice and stands.

## Adding a custom network

The person types the RPC URL and the chain id. The module (`network_custom`):

1. asks the RPC `getChannelId`. If it isn't the chain id typed, nothing changes: "That RPC serves
   chain 1111…1111, not the chain id you entered (2da7…5374). Nothing was changed.";
2. asks the RPC `getLastBlockId` (the height);
3. reads the network file: the address typed next to it, or by default
   `https://nameswallet.app/networks/<chain-id>.json`. The file must name the same chain id. For our
   own chain id a missing file (HTTP 404) is fine: the built-in ids stand;
4. shows what it found. Only "Switch to it" changes anything, and it makes every check again first.

The switch keeps the LEZ 0.4 CLI and wallet folder: the same recovery phrase, the same accounts. It
sets the RPC typed (never the file's `rpc`), the file's registry and bridges, adds its NFT programs
to Collectibles, and remembers the chain id. Home then never takes a sequencer of another chain
(`set_sequencer` and `zone_sequencer` refuse it); another chain is entered here again.

Choosing **LEZ 0.4 · Bridge test network** again goes back to the built-in RPC, registry and bridges.
The custom network is kept as `custom_network_last`, and the form offers it again.

## A reset that keeps the chain id

A chain started again with the same id is noticed by its height going down:

- **Check the network now** (Settings → Network) compares the height with the highest one this
  wallet saw on that chain (the custom network's record, and the bridge read's `bridge-net.json`);
- the balance read on the Wallet does the same every time it reads (`bridge_net::was_reset`).

Either way the wallet says so in a plain line ("…was reset at 14:02. What was held there before is
gone; everything shown was read again."), drops everything it had read (names, tokens, NFTs, bridge
Configs), and reads again. The reset is told for a day, and an approval made before it is not
signed again (`reset_refusal`). If the network file now lists other ids (a reset redeploys the
registry and the bridges), the check offers **Use the new ids**.

## The network file, `names-network/1`

A JSON object, UTF-8, at most 64 KiB, served over https (http only on this computer), with no
redirect. Published as `https://nameswallet.app/networks/<chain-id>.json`.

```json
{
  "format": "names-network/1",
  "name": "Bridge test network",
  "lez": "0.4",
  "chain_id": "2da7f7dfb6e51a28c942ea80391aa518253ec515e98d28cb4d83cc9e16ff5374",
  "rpc": "https://lez.nameswallet.app/",
  "registry": "6Yxj5k8AiuBSuMAYVQFGrYKjJGUSSaWWgoB9D9XXN3gA",
  "bridge_programs": ["JECKBkvoc2e3dC18Wm6zZJEC2BGMwUwLsdZiQzYTnUx7", "…"],
  "nft_programs": [{ "program": "<program id>", "label": "Name cards" }],
  "faucet": "https://lez.nameswallet.app/",
  "updated": "2026-11-19"
}
```

| Field | Required | Meaning and check |
|---|---|---|
| `format` | yes | Exactly `names-network/1`. Anything else is refused. |
| `name` | no | What the wallet calls the network. Cleaned (no control or bidi characters), at most 40 characters. Default "Custom network". |
| `lez` | yes | `"0.4"`. Only LEZ 0.4 networks can be added (they use the LEZ 0.4 names tool and wallet). |
| `chain_id` | yes | 64 hex characters: the chain's `getChannelId`. Must equal the chain id typed. |
| `rpc` | no | The network's public RPC, shown only. The wallet always uses the URL the person typed. |
| `registry` | yes | The LNS registry's program id (base58, 32 bytes). Not the native token. |
| `bridge_programs` | yes | 0 to 6 distinct bridge program ids (`Public/` allowed), read in this order. An empty list means the built-in bridges' ids, which a chain without them simply doesn't have. |
| `nft_programs` | no | Up to 16 `{program, label}`: added to Collectibles' list for LEZ 0.4. Labels are cleaned (40 characters). |
| `faucet` | no | Where the test-LEZ faucet answers `bridge_faucetInfo` / `bridge_faucet` (JSON-RPC). Default: the RPC. |
| `updated` | no | When the file was last changed. Not read by the wallet. |

Unknown fields are ignored, so later versions can add some without breaking this one.

## Our LEZ 0.4's file

`docs/wallet/networks/2da7f7dfb6e51a28c942ea80391aa518253ec515e98d28cb4d83cc9e16ff5374.json`, written
from the module's constants (`LEZ04_CHANNEL`, `LEZ04_SEQUENCER`, `LEZ04_REGISTRY`,
`bridge_net::DEFAULT_BRIDGE_PROGRAMS`; no NFT program is deployed there yet). The module test
`network::tests::our_network_file_is_the_built_in_network` fails if they drift apart. Publish it at
`https://nameswallet.app/networks/<chain-id>.json` (the conductor does this). After a reset of our
chain, update the file (new registry, bridges, and a new chain id if it changed) and publish it: a
wallet then reaches the new chain with no new build, through Settings → Network → Custom network (or
**Check the network now** → **Use the new ids** when the chain id stayed the same).

## Known limits

- A custom network shares LEZ 0.4's wallet folder (`lez-0.4/wallet`) with our own LEZ 0.4: the same
  keys on both. That is what a reset wants; two different LEZ 0.4 chains in use side by side are not
  supported.
- The LEZ Bridge module keeps its own deployment setting; a custom network's bridges are the
  wallet's reads and checks, not the bridge module's.
- The network file is trusted as far as the person trusts the address they typed (or
  nameswallet.app by default): every id in it is checked to be well formed, not to be the right one.
