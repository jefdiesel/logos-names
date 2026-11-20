# @lns/resolver

Resolve an LNS name against a LEZ sequencer and open what it holds. Read the
name's optional NFT (a "deed") from its record. Draw the holder's own name card
locally as SVG; the public NFT image never shows the name.

The name is a placeholder and the package is private for now.

## Install

From this folder, `npm install` fetches the one dependency (`@noble/hashes`,
the root's version). Import the modules by path, or by package name once
`@lns/resolver` is published:

```js
import { resolveName } from "@lns/resolver";
import { nameCard } from "@lns/resolver/card";
```

`RPC` is the sequencer's JSON-RPC URL and `PROGRAM` the registry's base58 account
id in the examples below. Both are passed in; the resolver does not fix a network.

## Examples

```js
import { normalizeName, resolveName, publicView } from "./names.js";
const name = normalizeName("Jef.lez");
const record = await resolveName(RPC, PROGRAM, name);
if (record) console.log(publicView(record)); // npk/vpk are not in this view
```

```js
import { resolveName } from "./names.js";
const record = await resolveName(RPC, PROGRAM, "jef");
console.log(record?.deed); // { definition, holding, nonce } or null
```

To derive the accounts from raw record bytes instead, read the record address,
`decodeNameRecord` it, then call `deedAccounts(program, id, rec.ownerTag, rec.deed.nonce)`.

```js
import { nameCard } from "./card.js";
import { writeFileSync } from "node:fs";
writeFileSync("jef.svg", nameCard("jefdiesel")); // a well-formed SVG string
```

For the returned fields, the exact errors, the forward and page helpers, and the
deed rules, see [AGENTS.md](AGENTS.md).
