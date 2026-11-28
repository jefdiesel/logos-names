#!/usr/bin/env node
// Proves names against the locked-name list the way the web app does (its shards, its
// prover), for `lns_js_vectors check-proofs` to verify with names_core's own verifier:
//   node shard-proofs.mjs <shards-dir | shortlist.bin> out.json name...
// A directory is read as the built shards (manifest.json + XX.bin, e.g. the hosted copy);
// a file is built into shards in memory first. Locked names are listed by id with the
// app's refusal.
import { readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { buildShardsFromLeaves, parseShortlistBin } from "../build-shortlist-shards.mjs";
import { createShardProver, encodeLengthProof } from "../../public/app/shortlist-shards.js";
import { deriveName, priceTierForLen } from "../../../resolver/names.js";

const [source, out, ...names] = process.argv.slice(2);
const hex = (a) => Buffer.from(a).toString("hex");
let fetchImpl;
if (statSync(source).isDirectory()) {
  fetchImpl = async (url) => new Response(readFileSync(join(source, String(url).replace("dir:/", ""))));
} else {
  const built = buildShardsFromLeaves(parseShortlistBin(new Uint8Array(readFileSync(source))));
  fetchImpl = async (url) => {
    const file = String(url).replace("dir:/", "");
    return file === "manifest.json" ? Response.json(built.manifest) : new Response(built.shards.get(file.replace(".bin", "")));
  };
}
const prover = createShardProver({ baseUrl: "dir:/", fetchImpl });
const result = { proofs: {}, locked: {} };
for (const name of names) {
  const { id } = await deriveName(name);
  try {
    result.proofs[name] = { id: hex(id), tier: priceTierForLen(name.length), proof: hex(encodeLengthProof(await prover.prove(id))) };
  } catch (error) {
    if (error.code !== "reserved") throw error;
    result.locked[name] = { id: hex(id), refusal: error.message };
  }
}
writeFileSync(out, JSON.stringify(result, null, 1));
console.log(`${Object.keys(result.proofs).length} proved, ${Object.keys(result.locked).length} refused as locked`);
