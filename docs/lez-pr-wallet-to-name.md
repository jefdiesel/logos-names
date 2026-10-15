PR title: `feat(wallet): pay a .lez name with --to-name`

PR from `jefdiesel/logos-execution-zone:feat/wallet-to-name` into `logos-blockchain/logos-execution-zone:dev`.

---

## 🎯 Purpose

Lets the official wallet pay a human-readable name: `wallet auth-transfer send --from <acct> --to-name jef.lez --amount 1000`. Today a payer has to get a 32-byte account ID, or a private account's npk/vpk, from the recipient by hand.

Names come from LNS (Logos Names), a name registry program deployed on the LEZ v0.3 testnet as `66nUvzJtTxA729aUpTcAWDrb7cV9Heb2bDzceHQhLYad` ([code and spec](https://github.com/jefdiesel/logos-names), [RFP suggestion](https://forum.logos.co/t/lns-lez-name-service-lez-names-private-by-default/2016)). Names are private by default: the chain never sees a name, only an identifier derived from it and a record sealed under a key derived from it, and a name normally publishes the npk/vpk of a private account, so paying it is a private transfer. A holder can also publish a public account instead.

## ⚙️ Approach

- [x] Add `lez/wallet/src/names.rs`: derive a name's identifier and key (PBKDF2-HMAC-SHA256, 100,000 rounds, then domain-separated SHA-256), find its record at `AccountId::for_public_pda(registry, sha256("logos-names/v2/name/" ‖ id))`, open the sealed profile (AES-256-GCM), check it is the name asked for and not lapsed, and return its private receiving keys or its public account.
- [x] Add `--to-name` and `--names-registry` (env `LEZ_NAMES_REGISTRY`) to `auth-transfer send`. `--to-name` conflicts with the other recipient flags; the recipient becomes `AccountIdentity::PrivateForeign { npk, vpk, kind: Regular }`, or `PublicNoSign` for a public account, and goes through the existing `NativeTokenTransfer::transfer`.
- [x] Add `pbkdf2 = "0.12"` to the wallet; `sha2` and `aes-gcm` were already workspace dependencies.

No registry ID is built into the wallet: it is passed with `--names-registry` or the environment. No core crate changes, so the RISC0 artifacts are untouched.

## 🧪 How to Test

```
cargo test -p wallet names::
```

The tests use a real record: `lez/wallet/tests/data/lns_jef_record.bin` is the record of `jef` fetched from the v0.3.1 testnet registry above. They check the specification's derivation vectors, the record address, that the record opens to a 1184-byte viewing key, and that a wrong name, an empty record and a lapsed name are refused.

`dev` can't talk to today's v0.3.1 testnet (its RPC returns `shards`, `dev` expects `actor_states`), so this can't be run end to end against the live chain until the testnet moves to a release that includes `dev`. The same resolution, in the LNS CLI built on v0.3.1, pays `jef.lez` privately on the testnet today.

## 🔗 Dependencies

None.

## 🔜 Future Work

- Name resolution anywhere else the wallet takes a recipient (only `auth-transfer send` here).
- A `names_registry` field in the wallet config, if maintainers prefer that to the flag and environment variable.
- A canonical registry for each network, once there is agreement on one.

## 📋 PR Completion Checklist

- [x] Complete PR description
- [x] Implement the core functionality
- [x] Add/update tests
- [x] Add/update documentation and inline comments
