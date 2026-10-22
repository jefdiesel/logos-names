# Run your own door (agent runbook)

A **door** is a Logos Delivery node that accepts browser WebSocket (`wss://`)
connections, so the Names webapp can reach the messaging network from a
browser. Point your own Claude Code (or any agent) at this file and it will stand one
up on your machine, prove it works, and hand you the address to paste into the app.

**Why run your own.** If everyone uses one shared door, that one node sees every
user's IP and which content topic they talk on — it becomes a single metadata choke
point and a single point of abuse/legal exposure. Running your own means *you* only
carry *your* traffic, and the default door's operator isn't carrying yours. This is
how the network decentralizes.

> **Eyes open — what a door sees.** Message *contents* are end-to-end encrypted; the
> door cannot read them. But the door (and whoever runs it) sees the **IP addresses**
> that connect and the **content topics** they use — i.e. who is messaging whom, when,
> not what. If you run a door for other people, you hold that metadata for them. If
> you *use* someone else's door, they hold it for you. Run your own, or use one you
> trust, and know which is which.

---

## What gets built (same for every path)

- A **Logos Delivery node** in `Core` mode on preset **`logos.dev`** — clusterId **3**,
  8 shards, RLN **off**, protocols relay + lightpush + filter, listening WebSocket on a
  local port (we use **8646**). This is the real server; it holds connections and
  relays messages.
- A **public `wss://` address** in front of that port, with a valid TLS cert and a
  **stable** hostname (a rotating address breaks the app — don't use a quick tunnel).
- A **persistent node key**, so the node's peer id never changes across restarts.
- A **service** so it survives reboot.

The finished address you paste into the app looks like:

```
/dns4/<your-host>/tcp/443/wss/p2p/<your-peerId>
```

### Two steps an agent CANNOT do for you (by design)

Agents are blocked from opening a public internet tunnel ("external ingress") and from
installing a boot service ("persistence") — these are yours to run, deliberately. When
the runbook reaches them it will **print the exact command/config and stop**, for you
to run. That's correct; don't try to work around it.

---

## Pick your path

- **A. You have the Logos node stack (`logosctl`).** Proven recipe below. Start here if
  you develop on Logos or already run a Logos node.
- **B. You run Basecamp.** Basecamp already runs a `delivery_module` — in principle you
  only need to turn WebSocket on and front it. **Experimental / unverified** (see notes).
- **C. A fresh Linux VPS.** Standalone node + your own domain. Most private (not on a
  home IP), costs a few $/mo.

Network constants every path shares:

```
preset        logos.dev
clusterId     3
numShards     8
ws port       8646   (any free port; 8646 is what the default door uses)
RLN           off
```

---

## Path A — with `logosctl` (proven)

This is exactly what runs the default door today.

**1. Make a config dir and start a daemon.**
```sh
N=~/names-door
LOGOSCTL=~/logos-node/logosctl-0.3.2/bin/logosctl     # adjust to your logosctl path
"$LOGOSCTL" --config-dir "$N" daemon start &
# wait until: "$LOGOSCTL" --config-dir "$N" daemon status  shows "status":"running"
```

**2. Install the delivery modules** (from your Logos catalog). You need
`libp2p_module`, `liblogos_lez_rln_module`, `liblogos_rln_module`, `delivery_module`.
```sh
"$LOGOSCTL" --config-dir "$N" install delivery_module    # pulls its deps; adjust if your
# catalog names differ. Package-signing warnings are harmless. Verify with:
"$LOGOSCTL" --config-dir "$N" module ls
```
> If your session has no catalog configured, set one with
> `logosctl catalog ...` first. (This is the one step not captured verbatim from the
> reference setup — an agent should use `logosctl search`/`install` to resolve it, and
> confirm all four modules load before continuing.)

**3. Create and start the node** (unload storage to avoid clashing with any storage
node you run):
```sh
CFG='{"mode":"Core","preset":"logos.dev","messagingOverrides":{"logLevel":"INFO","tcp-port":30322,"discv5-udp-port":9022,"websocket-support":true,"websocket-port":8646}}'
"$LOGOSCTL" --config-dir "$N" module unload storage_module 2>/dev/null || true
for m in libp2p_module liblogos_lez_rln_module liblogos_rln_module delivery_module; do
  "$LOGOSCTL" --config-dir "$N" module load "$m"; done
"$LOGOSCTL" --config-dir "$N" call delivery_module createNode "$CFG"
"$LOGOSCTL" --config-dir "$N" call delivery_module start
```
The node's key is persisted under `$N` (keyring + `data/delivery_module/`), so the peer
id is stable as long as `$N` survives. Get the peer id from the startup log
(`waku rendezvous ... peerId: 16Uiu2...`) or `getNodeInfo`.

→ Now go to **Verify**, then **Expose**, then **Make durable**.

---

## Path B — on top of Basecamp (experimental, unverified)

Basecamp already runs a `delivery_module`, so in theory you don't start a new node —
you just (1) get Basecamp's node to listen on a WebSocket port and (2) front that port
with `wss://`.

**Status: not yet verified.** Basecamp sets its own `createNode` config, and whether it
exposes a `websocket-support` override to users is unconfirmed. Do **not** follow fake
steps here. An agent on this path should:
- find Basecamp's delivery node config / `createNode` call and check for a
  `websocket-support` + `websocket-port` option,
- confirm the node's peer id is stable across Basecamp launches (Basecamp's *chat*
  address rotates; confirm the *delivery* node key does not),
- if both hold, enable WSS and jump to **Expose**; if not, fall back to **Path A** (run
  a separate door node beside Basecamp on a different port).

Report findings back so this section can be made real.

---

## Path C — fresh Linux VPS (most private)

**1. Get a delivery node binary.** Build or fetch `logos-delivery` (nwaku-family) from
`github.com/logos-messaging/logos-delivery`, or run `logosctl` on Linux as in Path A.
(If you build from source, pin a release; this step isn't captured verbatim here —
verify the node starts and prints its listen addrs.)

**2. Run it** with the same config as Path A (Core, logos.dev, cluster 3,
`websocket-support`, ws port 8646), key stored in a persistent dir.

**3. Expose it** with your own TLS instead of Cloudflare — e.g. Caddy (auto-cert):
```
msg.example.com {
    reverse_proxy 127.0.0.1:8646
}
```
Caddy terminates TLS and proxies the WebSocket. Point `msg.example.com`'s DNS at the
VPS. Then **Verify** (against `wss://msg.example.com`) and **Make durable** (systemd).

---

## Verify (every path — do this BEFORE going public)

Prove the node actually relays, locally, before exposing it. Needs Node.js.

```sh
mkdir door-check && cd door-check && npm init -y >/dev/null
npm i @waku/sdk@0.0.36 @libp2p/websockets
```
`check.mjs`:
```js
import { createLightNode, createEncoder, createDecoder, Protocols, utf8ToBytes, bytesToUtf8, utils } from "@waku/sdk";
import { webSockets } from "@libp2p/websockets";
import * as f from "@libp2p/websockets/filters";

const DOOR = process.argv[2];                 // e.g. /ip4/127.0.0.1/tcp/8646/ws/p2p/16Uiu2...
const NET = { clusterId: 3, numShardsInCluster: 8 };
const TOPIC = "/names/1/doorcheck/proto";
const ri = utils.createRoutingInfo(NET, { contentTopic: TOPIC });
const enc = createEncoder({ contentTopic: TOPIC, routingInfo: ri });
const dec = createDecoder(TOPIC, ri);
const local = DOOR.includes("/ws/") && !DOOR.includes("/wss/");   // plain ws => allow it
const waku = await createLightNode({ networkConfig: NET,
  ...(local ? { libp2p: { transports: [webSockets({ filter: f.all })] } } : {}) });
await waku.start(); await waku.dial(DOOR);
await waku.waitForPeers([Protocols.LightPush, Protocols.Filter], 20000);
let got = false;
await waku.filter.subscribe([dec], (m) => { got = true; console.log("RECEIVED:", bytesToUtf8(m.payload)); });
const r = await waku.lightPush.send(enc, { payload: utf8ToBytes("doorcheck " + Date.now()) });
console.log("lightpush:", JSON.stringify({ successes: r?.successes?.length, failures: r?.failures }));
await new Promise((x) => setTimeout(x, 8000));
console.log(got ? "OK — door relays" : "FAIL — no echo"); process.exit(got ? 0 : 1);
```
Run against the **local** port first:
```sh
node check.mjs /ip4/127.0.0.1/tcp/8646/ws/p2p/<your-peerId>
```
Green here = the node works. (For the public `wss://` address, drop the `ws`-only
override: a browser needs secure websockets with a valid cert, which is the point of
the next step.)

---

## Expose — YOUR step (agent prints, you run)

A stable `wss://` with a valid cert. Two common ways:

- **Cloudflare named tunnel** (what the default door uses): install `cloudflared`, run a
  named tunnel, and in the Cloudflare dashboard add a **Public Hostname**:
  `msg.<yourdomain>` → **HTTP** → `localhost:8646`. Cloudflare proxies the WebSocket and
  issues the cert. (Don't use `cloudflared --url` quick tunnels — the hostname rotates.)
- **Caddy/nginx on a VPS** (Path C): the `reverse_proxy` block above.

Either way, confirm `https://msg.<yourdomain>` reaches the node, then re-run **Verify**
against `/dns4/msg.<yourdomain>/tcp/443/wss/p2p/<your-peerId>`.

---

## Make durable — YOUR step

Keep it running across reboots.

- **macOS** — a LaunchAgent at `~/Library/LaunchAgents/local.names-door.plist` running a
  bring-up script (start daemon → load the four modules → `createNode` → `start`), with
  `RunAtLoad` + `KeepAlive`. (The reference setup's script is `~/names-door/run-door.sh`.)
- **Linux** — a `systemd --user` unit (or system unit) running the equivalent.

---

## Use it

In the Names webapp, open settings and set the **peer** field to your door's multiaddr:
```
/dns4/msg.<yourdomain>/tcp/443/wss/p2p/<your-peerId>
```
Messages now route through *your* door. Share the multiaddr with people you want to use
your door — or keep it to yourself.

> **Later:** the node already advertises `/logos/service-discovery/1.0.0`. When the app
> wires in service discovery (Logos issue #64), clients will be able to *find* community
> doors automatically, so no one is forced to run one and there's no default choke point.
