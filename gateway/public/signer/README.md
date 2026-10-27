# Names money-key signer

A self-contained page, served on its **own origin** (a separate subdomain in
production), embedded by the Names app as an iframe. It owns the wallet's secp256k1
key: generates it, encrypts it with a **passkey** (WebAuthn PRF → AES-GCM, `vault.js`)
in this origin's `localStorage`, and only ever decrypts it into memory for the span of
one signature. The private key **never** crosses `postMessage`.

Files here (each origin is self-contained, so `vault.js`, `lez-tx.js` and `protocol.js`
are copies of the app's — rebuild/copy them together):
- `index.html` + `signer.js` — the signer and its own confirmation UI.
- `protocol.js` — the postMessage envelope + validation (identical copy in `../app/`).
- `vault.js` — passkey vault (copied from `../app/vault.js`).
- `lez-tx.js` — LEZ signing lib (copied from `../app/lez-tx.js`).

## Production wiring

- Serve this directory on a **dedicated origin** (e.g. `https://signer.names.example`),
  distinct from the app's origin.
- In `signer.js`, set `PROD_PARENT_ORIGINS` to the app's exact origin. The `?parent=`
  dev override is ignored unless the signer is itself served from localhost/127.0.0.1.
- The app embeds the iframe with `allow="publickey-credentials-get;
  publickey-credentials-create"`. The passkey binds to THIS (signer) origin — correct,
  that's where the key lives.
- Set the app's client (`../app/wallet.js`) `origin` to this signer origin.

## Local two-origin test (real cross-origin isolation)

`localhost:8001` and `localhost:8002` are **different origins**, so this exercises the
origin checks for real. From `gateway/public/` serve the same tree on two ports:

```sh
cd gateway/public
python3 -m http.server 8001 &   # the APP origin
python3 -m http.server 8002 &   # the SIGNER origin
```

Open the app-side self-test on the app origin:

```
http://localhost:8001/app/signer-selftest.html
```

It defaults the signer URL to `http://localhost:8002/signer/index.html?parent=http://localhost:8001`
and the signer origin to `http://localhost:8002`. Click **Connect**, then:

1. **getAccount** → `{ account: null, canPersist: true }` before a key exists.
2. **createAccount** → the iframe overlay appears with its own "Create your wallet key"
   prompt; approving triggers the passkey. Returns `{ accountId, publicKey }`.
3. **getAccount** again → now returns the account.
4. Fund that `accountId` on the LEZ (faucet/CLI), then **signAndSend** with a `to`,
   an `amount`, and `rpc` `https://testnet.lez.logos.co` → the iframe shows "Approve
   this payment" with amount/recipient/fee; approve re-prompts the passkey, signs, and
   sends. Returns `{ hash, block }`.

To prove isolation, point a DIFFERENT page at the signer and confirm the signer ignores
it (origin not in the allowlist), and that no message ever contains the private key.

## What's verified vs not

- **Unit-tested in node** (`protocol.js` + `lez-tx.js` pure parts): envelope
  parse/build, origin-allowlist logic, `signAndSend` param validation, and that a signed
  transfer round-trips (sign → the account id matches the key).
- **Needs this live two-origin browser test** (WebAuthn/iframe can't run in node): the
  passkey create/get PRF round-trip, the cross-origin postMessage path, the overlay
  show/hide, and an actual on-chain send.
