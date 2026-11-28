# `lez_nft` v3 — Standalone LEZ v0.4.0 NFT Program

`lez_nft` v3 is a standalone non-fungible token and collection program for the **Logos Execution Zone (LEZ v0.4.0, RISC Zero 3.1.0)**. It combines:

- **PDA-per-token storage** — one account can hold unlimited NFTs without per-account binding limits.
- **Unlinkable `OwnerTag` ownership** — public token records never name a private holder in the clear.
- **Collections with public or private payees** — sequential minting (`serial = 1..=supply`) with native-token payment to a public account or a blinded private payee address.
- **Sealed content** — off-chain ML-KEM-768 + AES-256-GCM content sealing, resealable on transfer.
- **Opt-in LP-0001 privacy per NFT (`PrivateDeed`)** — any holder may shield a token into a private PDA readable via `Message::ReadState` for zero-knowledge collection-membership proofs, then unshield to transfer or burn.
- **Bidirectional authority linking (`LinkedAuthority`)** — couples 1-of-1 NFTs (such as LNS top-level names and permanent subnames) to an external registry (`names_core`) so transfers in either direction move both records atomically in one LEZ v0.4.0 transaction, with mint/burn/freeze/expiry/shield controlled exclusively by the linked authority.

See [SPEC.md](SPEC.md) for the complete protocol, PDA derivations, actor flows, and Borsh wire encodings.

## Workspace layout

| Crate | Path | Purpose |
|---|---|---|
| `lez_nft_core` | `core/` | Pure actor handler (`handle`), state & wire types, PDA helpers, unit tests, and Borsh vectors |
| `lez_nft_methods` | `methods/` | `risc0-build` wrapper embedding the compiled guest ELF & image ID |
| `lez_nft_guest` | `methods/guest/` | Thin RISC Zero guest (`lee_core::actor_entrypoint!(lez_nft_core::handle)`) |
| `lez_nft_cli` | `cli/` | `lez-nft` CLI built on the LEZ v0.4.0 `wallet` crate |

## Quick start (once compiled)

```bash
# Run core unit tests (including linked-authority flows, LP-0001 deeds, and Borsh vectors)
cargo test -p lez_nft_core

# Build guest ELF & CLI
cargo build --release -p lez_nft_methods -p lez_nft_cli
```

### Example CLI commands

```bash
# Initialize wallet (one public account + one private account)
lez-nft setup

# Admin: initialize program config authorizing the LNS registry and pinning its collection ID
lez-nft configure --from public --authority <LNS_REGISTRY_ID>:<LNS_COLLECTION_ID_HEX>

# Create a collection and mint serial #1
lez-nft create --content '{"name":"Genesis"}' --supply 100 --price 1000 --from public
lez-nft buy --collection <COLLECTION_ID_HEX> --from private

# Mint, shield (LP-0001), unshield, transfer, and burn a one-off NFT
lez-nft mint --content '{"name":"One-Off"}' --from private --encrypt
lez-nft shield-deed --token-id <TOKEN_ID_HEX>
lez-nft unshield-deed --token-id <TOKEN_ID_HEX>
lez-nft transfer --token-id <TOKEN_ID_HEX> --to <RECEIVE_CODE_OR_ACCOUNT>
lez-nft burn --token-id <TOKEN_ID_HEX>

# Transfer a linked LNS name NFT via lez-nft (declaring required registry PDAs)
lez-nft transfer --token-id <TOKEN_ID_HEX> --to <RECIPIENT> \
  --registry-actor <SETTINGS_PDA> --registry-actor <PAGE_PDA> --registry-actor <MARKET_BUCKET_PDA>
```
