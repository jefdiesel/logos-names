# Names Wallet 0.18

What changed since 0.17 (module, app and names CLI are all 0.18.0).

## Program calls can read the network clock

`propose_call` takes one account of another program, read only: the network
clock, written as

```json
{ "account": "4BdcjoXkq786TMWcBGGHqcxeLYMZmn17rL4eM9ZyRWNT",
  "program": "2hwaRW8sMnyKLLRbDRw8k68tiv9bnfq98oxcdtcp157Z" }
```

(LEZ v0.4.0-rc3 `clock_core`: the clock's account and the builtin clock
program). A program that reads the time (Vest's create, cancel, make
irrevocable, signal and transfer) needs it in the same transaction.

- The module takes only this exact entry: another account or another
  program, or the clock with `seed` or `balance`, is refused when the app
  asks. A stored request is checked again when it is approved.
- The prompt says "Reads the network clock (the time of the last block). It
  can't change the clock." The chain refuses any user transaction that writes
  the clock (LEZ `RestrictedAccountModification`), so that is true whatever
  the program does.
- The clock is never signed: the module marks it unsigned, and the CLI
  refuses a change that signs for it, gives it a seed, or names the clock's
  account or program in any other actor.
- The CLI (`module send`) reads the clock program's bytecode from the chain
  (cached next to the wallet as `lez-clock-<id>.bin`, `LOGOS_NAMES_CLOCK_ELF`
  overrides it) and adds it to the run that tries the call first and sizes
  its gas. A bare build sends it unchecked with its measured cap, as any other
  call.
- The module hands a call that reads the clock only to a names CLI that says
  0.18 or later; an older one would refuse it with "Program violated
  execution rules", so the module says what to install instead.
- `get_config` `features` has `call_reads_clock`.

## A fix in program calls

The chosen account's LEZ is given to a program only with
`{"account": "signer", "balance": true}`, which the prompt reports as "may
move LEZ out of the account you choose". An app that listed the chosen
account by its own ID for its balance got a prompt saying the call can't move
its LEZ, while LEZ 0.4 authorizes by account, so the program could have. The
module now refuses to sign such a request (nothing is sent; the app can ask
again with `signer`).

## Checked on our LEZ 0.4 (rc3, chain 2da7f7df…5374)

A Vest create through this path: Vest's `create_call` answer, the module's
`propose_call` and approval (its `module send` input captured), then the
0.18 CLI and a test wallet: 71,432 cycles, gas limit 107,148, applied.
