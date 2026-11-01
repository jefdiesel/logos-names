// Taking back a name held on the earlier chain (SPEC.md 8.4), as `names claim` does
// (cli/src/claims.rs, cli/src/main.rs). The snapshot is a static file, so nothing here
// reaches a server of ours, and it is checked against the root in the registry's settings
// before anything is signed.

import { decodeNameRecord, decodeSettings, deriveName, readActorState } from "./names.js";
import {
  accountIdOfPrivateKey, base58Decode, equalBytes, formatAccountId, fromHex, getAccountsNonces, sendTransaction,
  signPublicTransaction, toHex, waitForTransaction,
} from "./lez-tx.js";
import {
  accountBytes, claimLeaf, claimMessage, claimProof, claimRoot, ownerTag, pageAddress, recordAddress, setPageMessage, settingsAddress,
} from "./lns-tx.js";
import { chainNow, defaultRpc } from "./register.js";

export const SNAPSHOT_URL = "/claims/lez-0.3.json";

// Well above what the real guest used on the LEZ 0.4 engine: claim 65k, a small page 44k.
// A page's gas grows with its size, and the whole limit is only held, not spent.
const CLAIM_GAS = 150000n;
const PAGE_GAS = 2000000n;

const OLD_VERSION = 5;

/** `{ owner, sealed }` if `data` is exactly a record of the earlier chain, as strict borsh reads it. */
function oldRecord(data) {
  if (data.length < 45 || data[0] !== OLD_VERSION) return null;
  const length = new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(41, true);
  if (data.length !== 45 + length) return null;
  return { owner: data.slice(1, 33), sealed: data.slice(45) };
}

/** The sealed bytes of a page of the earlier chain, or null. */
function oldPage(data) {
  if (data.length < 5 || data[0] !== OLD_VERSION) return null;
  const length = new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(1, true);
  return data.length === 5 + length && length > 0 ? data.slice(5) : null;
}

const compare = (a, b) => {
  for (let i = 0; i < 32; i += 1) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return 0;
};

/**
 * Reads a snapshot file. The list has one leaf per record, sorted by address: the file also
 * holds settings, commitments and pages, which drop out because they aren't exactly a record.
 */
export function parseSnapshot(file) {
  const accounts = file.accounts.map(({ account, data }) => ({ account: base58Decode(account), data: fromHex(data) }));
  const records = accounts
    .flatMap(({ account, data }) => {
      const record = oldRecord(data);
      return record ? [{ account, ...record }] : [];
    })
    .sort((a, b) => compare(a.account, b.account));
  const leaves = records.map(({ account, owner }) => claimLeaf(account, owner));
  return { program: file.program, accounts, records, leaves, root: claimRoot(leaves) };
}

/** The proof for the name `id`, its record and its page's sealed bytes, or null if it isn't listed. */
export function claimFor(snapshot, id) {
  const address = recordAddress(accountBytes(snapshot.program), id);
  const index = snapshot.records.findIndex(({ account }) => equalBytes(account, address));
  if (index < 0) return null;
  const pageAt = pageAddress(accountBytes(snapshot.program), id);
  const page = snapshot.accounts.find(({ account }) => equalBytes(account, pageAt));
  return { proof: claimProof(snapshot.leaves, index), record: snapshot.records[index], page: page ? oldPage(page.data) : null };
}

async function send(rpc, signed) {
  await sendTransaction(rpc, signed);
  await waitForTransaction(rpc, signed.hash);
}

const failed = (what, tx) => new Error(`${what} was included in a block but failed, so nothing changed. Its fee was charged. (${tx})`);

/** Takes back `name` for the wallet of `privateKey`. Everything checkable is checked before anything is paid for. */
export async function takeBack({ privateKey, program, name, snapshotUrl = SNAPSHOT_URL, onStep = () => {} }) {
  const rpc = defaultRpc();
  const prog = accountBytes(program);
  const settingsBytes = await readActorState(rpc, program, formatAccountId(settingsAddress(prog)));
  if (settingsBytes.length === 0) throw new Error("This registry has no settings.");
  const settings = decodeSettings(settingsBytes);
  if (!settings.claimsRoot || /^0+$/.test(settings.claimsRoot)) throw new Error("This registry takes no claims.");
  const now = await chainNow(rpc);
  if (now >= settings.claimsUntil) throw new Error(`The claim period ended on ${new Date(settings.claimsUntil).toLocaleDateString()}.`);

  onStep("list");
  const response = await fetch(snapshotUrl);
  if (!response.ok) throw new Error("The list of names from the earlier chain isn't available yet.");
  const snapshot = parseSnapshot(await response.json());
  if (snapshot.program !== settings.claimsProgram || toHex(snapshot.root) !== settings.claimsRoot) {
    throw new Error("The list of names here isn't the one this registry's claims were made from.");
  }
  const { id } = await deriveName(name);
  const found = claimFor(snapshot, id);
  if (!found) throw new Error(`${name} isn't in the list of names from the earlier chain.`);
  const signer = accountIdOfPrivateKey(privateKey);
  if (!equalBytes(ownerTag(id, signer), found.record.owner)) throw new Error(`This wallet didn't hold ${name} on the earlier chain.`);
  const recordAccount = formatAccountId(recordAddress(prog, id));
  const held = await readActorState(rpc, program, recordAccount);
  if (held.length > 0 && decodeNameRecord(held).claimed) throw new Error(`${name} was already claimed.`);

  onStep("claim");
  const [nonce] = await getAccountsNonces(rpc, [signer]);
  const signed = signPublicTransaction(
    claimMessage({ program: prog, signer, nonce, gasLimit: CLAIM_GAS, id, sealed: found.record.sealed, now, proof: found.proof }),
    [privateKey],
  );
  await send(rpc, signed);
  const after = await readActorState(rpc, program, recordAccount);
  if (after.length === 0 || !decodeNameRecord(after).claimed || decodeNameRecord(after).ownerTag !== toHex(found.record.owner)) {
    throw failed("The claim", signed.hash);
  }
  // The claim emptied the page; the old one goes back in a transaction of its own.
  if (found.page) {
    onStep("page");
    const [next] = await getAccountsNonces(rpc, [signer]);
    await send(rpc, signPublicTransaction(
      setPageMessage({ program: prog, signer, nonce: next, gasLimit: PAGE_GAS, id, sealed: found.page }),
      [privateKey],
    ));
  }
  return { name, tx: signed.hash };
}
