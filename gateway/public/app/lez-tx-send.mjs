#!/usr/bin/env node
// Live check of lez-tx.js: a public native transfer signed in JavaScript, sent to a LEZ
// sequencer, and confirmed by getTransaction and the balances.
//
//   node lez-tx-send.mjs new <keyfile>                 make a signing key (hex, mode 600), print its account
//   node lez-tx-send.mjs account <keyfile>             print the account of a key
//   node lez-tx-send.mjs send <keyfile> [options]      send and confirm
//     --to Public/<id>    recipient (default Public/BJzLuDRny4wMmMCiJg8j3oajcv4wkZP9QcRNuefFKgn8)
//     --amount N          default 1000
//     --rpc URL           default https://lez.buggers.online (the Names test chain)
//     --gas-limit N       default 1000, as the names CLI uses for a native transfer
//     --preflight         send even when the balance can't cover it: expects "Incorrect fee" (the
//                         fee screen), which the node only reaches after decoding the transaction
//                         and verifying its signature
//
// Keep keyfiles out of public/: this file is only the script.
import { readFileSync, writeFileSync } from "node:fs";

import * as lez from "./lez-tx.js";

const DEFAULT_TO = "Public/BJzLuDRny4wMmMCiJg8j3oajcv4wkZP9QcRNuefFKgn8";
const DEFAULT_RPC = "https://lez.buggers.online";

const [command, keyfile, ...rest] = process.argv.slice(2);
const option = (name, fallback) => {
  const at = rest.indexOf(`--${name}`);
  return at >= 0 ? rest[at + 1] : fallback;
};
const flag = (name) => rest.includes(`--${name}`);
const readKey = (path) => lez.checkPrivateKey(lez.fromHex(readFileSync(path, "utf8").trim()));
const show = (accountId) => `Public/${lez.formatAccountId(accountId)}`;

async function send() {
  const rpcUrl = option("rpc", DEFAULT_RPC);
  const amount = BigInt(option("amount", "1000"));
  const gasLimit = BigInt(option("gas-limit", String(lez.NATIVE_TRANSFER_GAS_LIMIT)));
  const privateKey = readKey(keyfile);
  const from = lez.accountIdOfPrivateKey(privateKey);
  const to = lez.parseAccountId(option("to", DEFAULT_TO));
  if (lez.equalBytes(from, to)) throw new Error("sender and recipient are the same account");

  const quote = await lez.getFeeState(rpcUrl);
  const [nonce] = await lez.getAccountsNonces(rpcUrl, [from]);
  const [fromBefore, toBefore] = await Promise.all([lez.getAccountBalance(rpcUrl, from), lez.getAccountBalance(rpcUrl, to)]);
  console.log(`rpc       ${rpcUrl} (height ${quote.height}, base fees exec ${quote.base_fee_exec} / stor ${quote.base_fee_stor})`);
  console.log(`from      ${show(from)}  balance ${fromBefore}  nonce ${nonce}`);
  console.log(`to        ${show(to)}  balance ${toBefore}`);

  const maxFee = lez.maxFeeFor(gasLimit);
  const signed = lez.signTransfer({ privateKey, to, nonce, amount, gasLimit, tip: 0n, maxFee });
  const reserve = lez.feeReserve({ gasLimit, dataBytes: signed.wire.length, quote });
  console.log(`tx        ${signed.hash}  (${signed.wire.length} bytes; gas_limit ${gasLimit}, max_fee ${maxFee}, reserve ≈ ${reserve})`);
  if (reserve > maxFee) throw new Error(`max_fee ${maxFee} is below the reserve ${reserve}; raise --gas-limit`);
  if (fromBefore < amount + reserve) {
    const message = `${show(from)} holds ${fromBefore}; sending ${amount} needs about ${amount + reserve} (amount + fee reserve)`;
    if (!flag("preflight")) throw new Error(`${message}. Fund it, or pass --preflight to test the node's checks.`);
    console.log(`preflight ${message}; sending anyway`);
  }

  let returned;
  try {
    returned = await lez.sendTransaction(rpcUrl, signed);
  } catch (error) {
    console.log(`refused   ${error.message}`);
    if (flag("preflight")) {
      // The node checks decoding and signatures first ("Invalid params …", "Invalid signature(-s)"),
      // then the fee screen, whose refusals all read "Incorrect fee" (wallet-ffi maps it to
      // PayerCannotFund); with gas_limit and max_fee in range, an unfunded payer is what is left.
      const funded = /Incorrect fee/.test(error.message);
      console.log(funded ? "preflight OK: the node decoded the transaction and checked the signature and fee authorization; only the balance is missing." : "preflight: unexpected refusal; read the message above.");
      process.exit(funded ? 0 : 1);
    }
    throw error;
  }
  console.log(`accepted  ${returned}${returned === signed.hash ? "  (matches the local hash)" : `  (local hash ${signed.hash} DIFFERS)`}`);
  if (returned !== signed.hash) process.exitCode = 1;

  const block = await lez.waitForTransaction(rpcUrl, signed.hash, { onTick: (ms) => process.stdout.write(`\rwaiting   ${Math.round(ms / 1000)} s`) });
  process.stdout.write("\n");
  const [fromAfter, toAfter] = await Promise.all([lez.getAccountBalance(rpcUrl, from), lez.getAccountBalance(rpcUrl, to)]);
  const [nonceAfter] = await lez.getAccountsNonces(rpcUrl, [from]);
  console.log(`included  block ${block}`);
  console.log(`to        ${toBefore} -> ${toAfter} (${toAfter - toBefore >= 0n ? "+" : ""}${toAfter - toBefore})`);
  console.log(`from      ${fromBefore} -> ${fromAfter} (fee paid ${fromBefore - fromAfter - amount}); nonce ${nonce} -> ${nonceAfter}`);
  const credited = toAfter - toBefore === amount;
  console.log(credited ? "OK: the recipient was credited exactly the amount." : "CHECK: the recipient's balance did not move by exactly the amount (another payment, or the transfer failed in its block).");
  if (!credited) process.exitCode = 1;
}

try {
  if (command === "new") {
    const key = lez.randomPrivateKey();
    writeFileSync(keyfile, `${lez.toHex(key)}\n`, { mode: 0o600, flag: "wx" });
    console.log(show(lez.accountIdOfPrivateKey(key)));
  } else if (command === "account") {
    const key = readKey(keyfile);
    console.log(`${show(lez.accountIdOfPrivateKey(key))}  (x-only public key ${lez.toHex(lez.publicKeyOf(key))})`);
  } else if (command === "send") {
    await send();
  } else {
    console.log("usage: node lez-tx-send.mjs new|account|send <keyfile> [--to Public/<id>] [--amount N] [--rpc URL] [--gas-limit N] [--preflight]");
    process.exit(2);
  }
} catch (error) {
  console.error(`error: ${error.message}`);
  process.exit(1);
}
