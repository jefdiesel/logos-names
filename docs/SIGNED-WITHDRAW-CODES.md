# Signed withdraw-to codes (designed 2026-11-18, built 2026-11-19)

rfp-022 `proposal/WALLET-ROADMAP.md` item 9. **State: tested in unit and page tests only (2), 2026-11-19.**
Built on rfp-022 branch `roadmap-2` (the page's "Sign the code with your wallet", the module's
`decode_withdraw_to_signed`, vectors `web/deposit/test/vectors/withdraw-signed-v1.json`; its
`modules/NAMES-SEND-BACK.md` §3.1d is the record) and here on `names-roadmap-2` (the line under the four
words, `Logic.signatureLine`). The page is not deployed. The owner's answers to §7 (2026-11-19, through
the task): unsigned codes and pasted addresses keep working, with "Not signed: compare the four words";
a signature that is present but wrong is refused. The text below is the design as written before.

## 1. What it is for

Today Bridge to Ethereum takes a `logoswd1…` withdraw-to code (NAMES-SEND-BACK.md §3.1), which the bridge
page shows for its connected Ethereum wallet, or a pasted `0x…` address (§3.1a). The person then compares
four words on both screens.

A signed code adds one thing: **the code carries a signature from the address it pays to.** The wallet
refuses a code whose signature doesn't recover that address. That catches, without a human step:
- a code that was mistyped, mangled or cut short (bech32m already catches most of these);
- a code edited to another recipient by hand or by a tool;
- a code made for an address whose key the page's connected wallet doesn't hold.

**What it does not catch:** a whole code swapped for another, validly signed one. An attacker who can
replace what the person pastes can sign their own code with their own wallet. The four-word comparison
against the page, with the person's own wallet connected, stays a **required** step (the tick "This is my
address" on step 2). The signature only removes the mistakes nobody meant.

## 2. The message signed

**Signing:** EIP-191 `personal_sign` (version `0x45`) by the connected Ethereum account, on the bridge page,
once after Connect.

**The text** is plain ASCII. Each line is fixed except the values. Lines end in `\n` and there is no
trailing newline:

```
Logos bridge: withdraw-to code
Domain: logos-bridge/withdraw-to/v2
Chain: 11155111
Vault: 0xAFbDF192434E9888Bb23DAE6f2D15Eea6246eD10
Address: 0x68B54942A37aE5ceBA34D47eFB8d33a293d3975d
Relay fee choice: 0
Only sign this on the bridge page, for an address that is yours.
```

**What each part does:**
- **The domain tag** `logos-bridge/withdraw-to/v2` separates this statement from any other message the same
  key might sign.
- **The chain id and the vault** bind the statement to one deployment. A signature for v4's vault does not
  verify for v5's.
- **The address** is the recipient. Both the page and the module write it in its EIP-55 form, so the bytes
  signed are fixed.
- **The relay fee choice** (0 to 4) is signed too. A code that names a fee the person didn't choose is
  refused like a changed address.
- **The last line** is for the person reading the wallet's sign prompt. It is part of the signed bytes.
- **No nonce and no time.** The statement is the same each time: "withdrawals from this vault may go to this
  address, with this fee choice". Replaying it is harmless. Leaving them out also keeps the code
  reproducible after a restore (NAMES-SEND-BACK.md §3.1).

**The digest:**

```
digest = keccak256("\x19Ethereum Signed Message:\n" + decimal(len(text)) + text)
```

`len` counts the text's bytes.

## 3. The code, version 2

```
payload v2 = version(1)=2 | chainId(8, big-endian) | vault(20) | recipient(20) | relayFeeChoice(1)
             | signature(65: r 32 | s 32 | v 1)
code       = bech32m("logoswd", payload)        115 bytes, about 198 characters
link       = https://nameswallet.app/bridge/#logoswd1…
```

- **The signature:** `v` is 27 or 28 (the page normalises 0/1 to 27/28), and `s` must be in the lower half
  of the curve order (EIP-2).
- **The four words do not change.** They stay the compare words of
  `SHA-256("logos-bridge/withdraw-to/v1" | payload v1)`, over the **unsigned** fields only.
  - The same address on the same vault gives the same words, signed or not.
  - The words in the address book, the page and the release code stay as they are.
  - The two signatures of one address (wallets need not sign deterministically) give one set of words.
- **v1 codes stay valid** at the format level. What the wallet does with one is an open question for the
  owner (§7).

## 4. What the wallet verifies (in the LEZ Bridge module)

Verification lives in the module (`withdraw.rs`), not in QML. `send_back_check(code)` and `send_back_plan`
both verify again; neither trusts the other.

1. Decode the code as today: `bad_code:*`, `other_chain`, `other_vault`, `bad_recipient`, `bad_fee_choice`.
2. For version 2:
   1. Rebuild the text of §2 from the decoded chain id, vault, recipient and fee choice.
   2. Compute the digest.
   3. Recover the signer: secp256k1 `ecrecover`, refusing a high `s` or a `v` not 27/28.
   4. Take the address as the last 20 bytes of `keccak256(pubkey)`.
3. **Refusals:**
   - `bad_signature`: it doesn't recover. Words: "This code's signature is broken. Copy the code from the
     bridge page again."
   - `signer_mismatch`: it recovers another address. Words: "This code wasn't signed by the address it pays
     to. Don't use it; copy the code from the bridge page again."
   - Nothing is stored on either refusal.
4. **On success** the reply gains `signed: true` and `signer` (the same address). A v1 code answers
   `signed: false`.

**Test vectors:** a generator in Python standard library only, beside `tools/make-withdraw-vectors.py`. It
needs:
- the Keccak-256 the EIP-55 generator already has;
- a small secp256k1 sign and recover, from fixed test keys, never a real key.

The vectors are read by `withdraw.rs` and by the page's `withdraw.test.mjs`. They cover:
- good codes;
- one bit flipped in each field;
- a high `s`;
- `v` 0/1 vs 27/28;
- another vault;
- another fee choice.

## 5. What the page does

- **In `#withdraw` mode, after Connect:** one `personal_sign` request with the text of §2, then the v2 code
  and the same four words.
- **If the person declines the signature:** the page says so plainly. Whether it then shows a v1 code is the
  owner's call (§7).
- **The page checks its own code** with the same verifier before showing it (a wallet that signed with
  another account is caught on the page).

## 6. What stays manual, and the Names app

- **Manual:** comparing the four words with the page while the person's own wallet is connected, and the
  tick "This is my address". The signature proves the code came from a key for the address. It doesn't
  prove the address is the person's.
- **The Names app** shows one more line on step 2 when `signed: true`: "Signed by this address on the bridge
  page." For a refusal it shows the module's words. Nothing else changes: the module verifies.
- **Privacy note:** a signature reveals the account's public key. An Ethereum account that never sent a
  transaction has not published it yet. This is harmless today, but worth one line in the page's details.

## 7. Open questions for the owner (not decided here)

1. **Unsigned v1 codes:** keep them with a warning for a while, or refuse them once the page signs? Refusing
   breaks codes people saved or sent each other.
2. **The pasted-address route** (NAMES-SEND-BACK.md §3.1a, for a phone wallet that can't hand a code to a
   Mac): it can carry no signature.
   - Keep it, with the four words and the tick as its only check?
   - Or require the page's signed code once signing exists?
3. **Saved addresses:**
   - Should an address saved from a signed code be marked as such?
   - Should one saved from a pasted address ask for a signed code the first time it is used after the
     change?
4. **Contract wallets** (Safe and other EIP-1271 accounts): `ecrecover` can't verify them.
   - Checking `isValidSignature` needs an `eth_call` on Sepolia through the module's RPCs, which can lie
     (the module trusts them for other reads already).
   - Or refuse contract recipients on the signed path and leave them to the pasted-address route.
5. **Whether the relay fee choice belongs in the signed text at all,** if the fee choice moves to the wallet
   (it is 0 today on every code the page makes).
