Real replies of the bridge test network (LEZ 0.4, rc1) at https://lez.nameswallet.app/, recorded
2026-11-13 (blocks 512-570) with plain curl, byte for byte. `config.json` is `getAccount` of the
bridge Config PDA `HugSoWM7Hp1eXMqknfCewWNMoraweLG3HkezfNGtqKf3`; `def-*.json` of the wrapped ETH and
dUSDC definitions; `owner.json` of `CCVd76Tk3WvFb63Wbuu4exMeLRTbSuMWHqxuvysapKnk` (0.1 wrapped ETH,
23,786,858 native); `operator.json` of the bridge operator; `unseen.json` of an id the chain has
never seen (`null`). How to read them: public-demo/WALLET-READ.md in the bridge repository.

`config-v4.json`: `getAccount` of the v4 bridge's Config PDA `HUdVSAHEKE2cQbrzzb9eBi4LSwjvQV64XqxmwH9PXsWX`
(bridge `BKXe3q5JcKeiSrKv9FUBjZw3AowbvYgyHZ3SYPgDxJBf`, vault `0xC2c6A059…54Ed`), read 2026-11-15 with
one read-only `getAccount` (python urllib). Its token program is v3's (`5w6nTkft…ArbU7`); wrapped ETH
`9tcLmjKixcGZJMnXS381Dhuzd8u9jPvQYF21F3vDhGE2` (denominations 0.01, 0.1, 1 ETH) and dUSDC
`92PJXZ2E3QPupUFuhc5f4rgsRiAZKv62TpSYfZEgLeru`; burns not frozen. The send-back burn checks read it.

`config-v5.json`: `getAccount` of the v5 bridge's Config PDA `FBWWSLgMgEtMs7PUNBfTGXccZdRSAeRHtMRP4nyZsrnW`
(bridge `CuV1ctZ2qJG4wQcXP4TBudVGKzNmdoYT3DpP1veHnDMA`, vault `0xAFbDF192…eD10`, no release delay), read
2026-11-15 evening the same way. Token program v3's and v4's (`5w6nTkft…ArbU7`); wrapped ETH
`Dx5tbzn54vKJiMYmjuYV3uuhi4eNwqnzZys7EqT3gj9z` (0.01, 0.1, 1 ETH), dUSDC `8ctGbFACndG9C6AjzN34gBpi6kRcDhv9guA2sxBsXomT`.
