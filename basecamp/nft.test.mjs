// Checks Collectibles' rules (qml/nft.mjs). Run with: node basecamp/nft.test.mjs
import { capabilityTitle, VPK_HEX, leftLine, splitRunningJobs, payerLinkLine, listAccounts, tickedFor, pruneTicks, proofLine, proofKind, proofPublicLine, proofGoodToKnow, proofChecksLine, capabilityVerb, readyRows, privateOnly, accountShown, idShown, tokenLine, collectionTitle, metadataRows, accountGroups, collectionCounts, receiveChoices, collectionLink, linkOf, toProblem, toTheirKeys, approveWhy, defaultPayer, jobLine, jobEnded, imageConsent, burnConfirm } from "./qml/nft.mjs";
import { readFileSync } from "node:fs";

let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};

const PROGRAM = "ErycMYqXUUtducaYmpHZjRgdbLXchrMiV4NZRnmRDNmu";
const ART = "HA47ERcmfSbDChGcYfBcC4SKaPQ8pJJnv4wxCLu1ejL5";
const BOB = "HLEwXNBDYx6GS9H5CsawDrMTptD6R4XsJywFEMr6eim2";
const art = { definition: ART, name: "Art", printable_supply: "3", standard: "Simple", uri: "ipfs://bafyart/meta", creators: "me" };

check("idShown", idShown(ART), "HA47ER…1ejL5".replace("1ejL5", "ejL5"));
check("accountShown", [accountShown("Private/" + BOB), accountShown(BOB)], ["Private HLEwXN…eim2", "HLEwXN…eim2"]);
// No serial: the master or a copy, and what the master can still print.
check("tokenLine", [tokenLine({ state: "copy" }), tokenLine({ state: "master", print_balance: "3" }), tokenLine({ state: "master", print_balance: "2" }), tokenLine({ state: "master", print_balance: "1" })],
  ["Copy", "Master · can print 2 more copies", "Master · can print 1 more copy", "Master · no prints left"]);
// Missing metadata is said, never hidden.
check("collectionTitle", [collectionTitle(art, ART), collectionTitle(null, BOB), collectionTitle({ name: " " }, BOB)], ["Art", "Unreadable collection HLEwXN…eim2", "Unreadable collection HLEwXN…eim2"]);
check("metadataRows", metadataRows({ ...art, creators: "" }).map((r) => r.value), ["3", "Simple", "ipfs://bafyart/meta", "none given"]);
check("metadataRows none", metadataRows(null)[0].label, "Metadata");

const programs = [{
  program: PROGRAM, label: "Throwaway",
  holdings: [
    { account: "Public/" + BOB, account_kind: "public", state: "copy", holds: true, definition: ART, collection: art },
    { account: "Private/" + ART, account_kind: "private", state: "master", holds: true, definition: ART, print_balance: "2", collection: art },
    { account: "Public/" + PROGRAM, account_kind: "public", state: "empty", holds: false },
    { account: "Public/X", account_kind: "public", state: "copy", holds: false, definition: ART },
    { account: "Public/Y", account_kind: "public", state: "unreadable", holds: false, unreadable: "it ends too early" },
  ],
  collections: [{ definition: ART, name: "Art", readable: true, count: 2, accounts: ["Public/" + BOB, "Private/" + ART] }],
}];
const groups = accountGroups(programs);
check("accountGroups: holders and the unreadable one", groups.map((g) => [g.account, g.isPrivate, g.nfts.length]), [["Public/" + BOB, false, 1], ["Private/" + ART, true, 1], ["Public/Y", false, 1]]);
check("collectionCounts", collectionCounts(programs), [{ program: PROGRAM, definition: ART, title: "Art", count: 2, accounts: ["Public/" + BOB, "Private/" + ART] }]);
check("receiveChoices", receiveChoices(programs, PROGRAM, ART), { empty: ["Public/" + PROGRAM], ready: ["Public/X"] });

check("collectionLink link", collectionLink(linkOf(PROGRAM, ART), ""), { program: PROGRAM, definition: ART });
check("collectionLink id", collectionLink(ART, PROGRAM), { program: PROGRAM, definition: ART });
check("collectionLink id without program", collectionLink(ART, "").problem !== undefined, true);
check("collectionLink junk", collectionLink("https://x", PROGRAM).problem !== undefined, true);

check("toProblem good", ["alice", "alice.lez", "Public/" + BOB, "Private/" + BOB, "ab".repeat(32) + ":" + "cd".repeat(1184)].map(toProblem), ["", "", "", "", ""]);
// Real receiving keys: a 1184-byte vpk. Any other length is refused here as in the module.
check("toProblem vpk length", [10, 256, 1183, 1185].map((n) => toProblem("ab".repeat(32) + ":" + "cd".repeat(n)) !== ""), [true, true, true, true]);
check("VPK_HEX", VPK_HEX, 2368);
check("toProblem bad", ["", BOB, "Public/x!", "ab:cd", "two words"].map((t) => toProblem(t) !== ""), [true, true, true, true, true]);
check("toTheirKeys", [toTheirKeys("ab".repeat(32) + ":cd"), toTheirKeys("alice")], [true, false]);

// The sheet's button waits for each needed choice, and says which.
const sheet = { action: "send", proven: true, payers: [{ account: "Private/A", enough: false }, { account: "Private/B", enough: true }], warnings: ["x"] };
check("approveWhy no sheet", approveWhy(null, {}), "Checking what this does…");
check("approveWhy payer", approveWhy(sheet, {}), "Choose the private account that pays the fee.");
check("approveWhy poor payer", approveWhy(sheet, { payer: "Private/A" }), "That account can't cover the fee; choose another.");
check("approveWhy warnings", approveWhy(sheet, { payer: "Private/B" }), "Tick the box above to go ahead.");
check("approveWhy ready", approveWhy(sheet, { payer: "Private/B", warnings: true }), "");
check("approveWhy no private payer at all", approveWhy({ proven: true, payers: [{ account: "Private/A", enough: false }] }, {}).includes("Move some LEZ"), true);
check("approveWhy public payer", [approveWhy({ proven: false, payers: [{ account: "Public/A", enough: true }], warnings: [] }, {}), approveWhy({ proven: false, payers: [{ account: "Public/A", enough: true }], warnings: [] }, { payer: "Public/A" })], ["Choose the account that pays the fee.", ""]);
check("approveWhy burn", approveWhy({ action: "burn", proven: false, warnings: [] }, {}), "Tick that you understand it can't be undone.");
check("approveWhy refused shows the refusal instead", approveWhy({ refusal: "That account already holds an NFT of this collection." }, {}), "");
check("defaultPayer", [defaultPayer(sheet, "Private/A"), defaultPayer(sheet, "Private/B"), defaultPayer({ payers: [] }, "")], ["Private/B", "Private/B", ""]);

check("jobLine public", jobLine({ label: "Sending Art", proves: false, started: 0 }, 12000), "Sending Art: sending it publicly, one block (about 30 seconds), 12s so far.");
check("jobLine proven", jobLine({ label: "Sending Art", proves: true, started: 0 }, 180000).includes("7-12 minutes"), true);
check("jobEnded done", jobEnded({ label: "Sending Art", kind: "nft_send" }, { state: "done", block: 7 }), { kind: "done", text: "Sending Art: done, in block 7." });
check("jobEnded refused", jobEnded({ label: "Sending Art", kind: "nft_send" }, { state: "failed", refusal: "That account isn't ready for this collection: it has to receive a holding first." }).kind, "problem");
check("jobEnded init gives the account to copy", jobEnded({ kind: "nft_init", label: "x" }, { state: "done", block: 4, account: "Public/" + BOB }), { kind: "done", text: "Ready: Public HLEwXN…eim2 takes this collection now (block 4). Give the sender this account.", copy: "Public/" + BOB });
check("readyRows", readyRows(programs).map((r) => r.account), ["Public/X"]);
check("jobEnded receive", [jobEnded({ kind: "nft_receive" }, { state: "done", nothing: true }).text, jobEnded({ kind: "nft_receive" }, { state: "done", received: 1 }).text], ["There was nothing to receive.", "Received 1 NFT."]);

check("privateOnly", [privateOnly(programs, PROGRAM, ART), privateOnly([{ program: PROGRAM, holdings: [programs[0].holdings[1]] }], PROGRAM, ART)], [false, true]);
check("imageConsent non-https", imageConsent("ipfs://bafy", false).allowed, false);
check("imageConsent https names the host", imageConsent("https://img.example.com/a.png", true).text.includes("img.example.com"), true);
check("imageConsent private warns", imageConsent("https://img.example.com/a.png", true).text.includes("Only private accounts"), true);
check("burnConfirm", [burnConfirm({ state: "copy" }).includes("this copy"), burnConfirm({ state: "master" }).includes("master")], [true, true]);

// Membership proofs: the stages, with the time spent proving; named errors.
check("proofLine proving", proofLine({ state: "proving", elapsed_seconds: 125 }), "Proving (about 10 minutes): 2 min 5 s so far.");
check("proofLine failed", proofLine({ state: "failed", code: "GATE_NOT_PINNED", plain_message: "No gate." }), "Didn't go through: No gate. (GATE_NOT_PINNED)");
check("proofLine ready", proofLine({ state: "ready", proof: { expires_at: Date.UTC(2026, 10, 19, 12, 0) } }), "Ready: the pass is valid until 2026-11-19 12:00 UTC.");
check("proofKind", ["ready", "failed", "proving", "cancelled"].map((s) => proofKind({ state: s })), ["done", "problem", "waiting", ""]);
check("proofPublicLine", proofPublicLine("When the pass starts and ends: until " + Date.UTC(2026, 10, 19, 13, 0) + "."), "When the pass starts and ends: until 2026-11-19 13:00 UTC.");
check("proofGoodToKnow names the app", proofGoodToKnow({ good_to_know: ["a", "b"], live_passes: [{ app: "door_ui" }] })[1].includes("door_ui"), true);
check("proofChecksLine", proofChecksLine({ gate: null, nft_program_label: { registry: false } }), "Program checks: gate none set up on this network yet, NFT program unknown (no Program Registry).");
check("capabilityVerb", ["nft_list", "nft_transfers", "membership_proof"].map(capabilityVerb), ["see your NFTs", "propose NFT transfers", "ask you for membership proofs"]);

// Review round 6.
check("jobEnded receive with one left waiting", jobEnded({ kind: "nft_receive" }, { state: "done", received: 0, left: [{ account: "Private/" + BOB, code: "not_ready" }] }).kind, "problem");
check("jobEnded says the next step", jobEnded({ kind: "nft_receive" }, { state: "done", received: 1, left: [{ account: "Private/" + BOB, code: "already_holds" }] }).text.startsWith("Received 1 NFT. 1 is still waiting for Private HLEwXN…eim2: that account already holds"), true);
check("leftLine other collection offers no retry", leftLine({ account: "Private/" + BOB, code: "other_collection" }).includes("can never take"), true);
check("leftLine not ready says make it ready", leftLine({ account: "Private/" + BOB, code: "not_ready" }).includes("Make Private HLEwXN…eim2 ready"), true);
const split = splitRunningJobs([{ id: "-wallet-1", name: "-wallet", kind: "nft_send", started: 5 }, { id: "x", name: "jef", kind: "post" }]);
check("splitRunningJobs: an NFT job goes to Collectibles", [split.nft.id, split.nft.label, split.rest.map((j) => j.id)], ["-wallet-1", "Your NFT send", ["x"]]);
check("splitRunningJobs: none", splitRunningJobs([{ id: "x", kind: "post" }]).nft, null);
check("jobLine of a picked-up job", jobLine({ label: "Your NFT send", proves: undefined, started: 0 }, 120000), "Your NFT send: under way, 2 min so far. You can keep using Names meanwhile.");
check("payerLinkLine", [payerLinkLine({ proven: false }, { payer: "Public/F" }, ""), payerLinkLine({ proven: false }, { payer: "Public/F" }, "Public/F"), payerLinkLine({ proven: true }, { payer: "Private/P" }, "")].map((l) => l !== ""), [true, false, false]);
check("listAccounts: the union over programs", listAccounts([{ program: "a", error: "x" }, { program: "b", holdings: [{ account: "Public/1", account_kind: "public" }, { account: "Private/2", account_kind: "private" }] }, { program: "c", holdings: [{ account: "Public/1" }] }]).map((r) => r.account), ["Public/1", "Private/2"]);
check("tickedFor: only this request's, only those shown", tickedFor({ r1: { "Private/2": true, "Public/9": true }, r2: { "Public/1": true } }, "r1", [{ account: "Private/2" }, { account: "Public/1" }]), ["Private/2"]);
check("tickedFor: a new request starts empty", tickedFor({ r1: { "Private/2": true } }, "r3", [{ account: "Private/2" }]), []);
check("pruneTicks drops requests gone", pruneTicks({ r1: { a: true }, r2: { b: true } }, [{ id: "r2" }]), { r2: { b: true } });
check("imageConsent host must be a plain name", [imageConsent("https://<b>x</b> y/", false).allowed, imageConsent("https://img.example.com:8443/a.png", false).allowed], [false, true]);

check("capabilityTitle", [capabilityTitle({ capability: "nft_list", accounts: ["Public/" + BOB] }), capabilityTitle({ capability: "membership_proof" }).includes("nothing about your accounts")], ["NFT list: Public HLEwXN…eim2", true]);

// The words the wallet never uses, in the new code and the Collectibles block of Main.qml.
const source = readFileSync(new URL("./qml/nft.mjs", import.meta.url), "utf8");
const main = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
const block = main.slice(main.indexOf("// ── Collectibles ──"), main.indexOf("// ── end Collectibles\n")) + main.slice(main.indexOf("// ── Collectibles page"), main.indexOf("// ── end Collectibles page"));
check("Collectibles block present", block.length > 1000, true);
for (const [label, text] of [["nft.mjs", source.split("\n").filter((l) => !l.startsWith("//")).join("\n")], ["Main.qml Collectibles", block]]) {
  check(label + ": no 'lookup' / 'official'", /\blook ?up\b|\bofficial\b/i.test(text), false);
}
// Every Text in the Collectibles blocks says its format (an app's or a chain's text is never markup).
// Each Text element's own body, matched brace for brace, so a neighbour's format never counts.
const elements = [];
for (const m of block.matchAll(/\bText \{/g)) {
  let depth = 0, i = m.index + m[0].length - 1, end = i;
  for (; end < block.length; end++) {
    if (block[end] === "{") depth++;
    else if (block[end] === "}" && --depth === 0) break;
  }
  elements.push(block.slice(i, end + 1).replace(/\{[^{}]*\{[\s\S]*\}[^{}]*\}/g, ""));
}
check("Collectibles Text elements found", elements.length > 5, true);
check("every Collectibles Text sets textFormat", elements.filter((body) => !/textFormat:/.test(body)).length, 0);
const hint = main.slice(main.indexOf("    component Hint: Text {"), main.indexOf("    component Hint: Text {") + 300);
check("Hint is plain text by default", /textFormat: Text.PlainText/.test(hint), true);

// No polling while the page is closed: every NFT timer runs only while Collectibles is open.
const timers = block.split("Timer {").slice(1).map((t) => t.slice(0, 400)).filter((t) => /repeat: true/.test(t));
check("every Collectibles timer needs the page open", timers.length > 0 && timers.every((t) => t.includes('root.section === "collectibles"')), true);
check("every Collectibles timer is 30 s or slower", timers.every((t) => Number((/interval: (\d+)/.exec(t) || [0, 0])[1]) >= 30000), true);

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures === 0 ? 0 : 1);
