// Checks Collectibles' rules (qml/nft.mjs). Run with: node basecamp/nft.test.mjs
import { capabilityTitle, VPK_HEX, MAX_MEDIA_URI, DEFAULT_STORAGE_URL, privacyLabel, parseMediaUri, mediaState, errorIsUnverified, storageNodeProblem, collectionCards, leftLine, splitRunningJobs, payerLinkLine, listAccounts, tickedFor, pruneTicks, proofLine, proofKind, proofPublicLine, proofGoodToKnow, proofChecksLine, capabilityVerb, readyRows, privateOnly, accountShown, idShown, tokenLine, collectionTitle, metadataRows, accountGroups, collectionCounts, receiveChoices, collectionLink, linkOf, toProblem, toTheirKeys, approveWhy, defaultPayer, jobLine, jobEnded, imageConsent, burnConfirm } from "./qml/nft.mjs";
import { itemOf, deedNameOf, isDeed, itemDescription, itemFields, editionLines, registryLine, ownerShown, toName, nameStep, namePreview, itemSendWhy, GRACE_MS } from "./qml/nft.mjs";
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
// Deep review B10: lns:alice and @alice are names (SPEC.md §2), never receiving keys.
check("toProblem: lns: and @ names", ["lns:alice", "LNS:Alice.lez", "@alice", "lns:", "lns:two words"].map((t) => toProblem(t) === ""), [true, true, true, false, false]);
check("toTheirKeys: lns:alice is a name", [toTheirKeys("lns:alice"), toTheirKeys("@alice")], [false, false]);

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


// ── The item page ──
const item = { program: PROGRAM, account: "Public/" + BOB, definition: ART };
const listed = [{ ...programs[0], registry: { registry: true, text: "Art program", verified: true } }];
check("itemOf: the held NFT, with its program's registry fields", (({ account, definition, state, programLabel, registry, isPrivate }) => ({ account, definition, state, programLabel, registry, isPrivate }))(itemOf(listed, item)),
  { account: "Public/" + BOB, definition: ART, state: "copy", programLabel: "Throwaway", registry: { registry: true, text: "Art program", verified: true }, isPrivate: false });
check("itemOf: gone once it no longer holds it", [itemOf(programs, { ...item, account: "Public/X" }), itemOf(programs, { ...item, program: "other" }), itemOf(programs, null), itemOf(null, item)], [null, null, null, null]);
check("itemOf: an unreadable one by its account", itemOf(programs, { program: PROGRAM, account: "Public/Y", definition: "" }).state, "unreadable");
const deeds = { grace05: { deed: { definition: ART, nft_program: PROGRAM } }, turing: { deed: null }, ada: {} };
check("deedNameOf", [deedNameOf(deeds, PROGRAM, ART), deedNameOf(deeds, "other", ART), deedNameOf(deeds, PROGRAM, BOB), deedNameOf(null, PROGRAM, ART), deedNameOf(deeds, PROGRAM, "")], ["grace05", "", "", "", ""]);
// Deep review B11: anyone can name a collection "LNS name deed"; a card is one of the names' deeds
// only (a real deed sits in a registry account, never in a wallet account's holdings).
check("isDeed: a name's card only, never a collection that is just called so",
  [isDeed(deeds, [], PROGRAM, ART, art), isDeed({}, [{ program: PROGRAM, built_in: true }], PROGRAM, BOB, { name: "LNS name deed" }), isDeed({}, [{ program: PROGRAM }], PROGRAM, BOB, { name: "LNS name deed" }), isDeed({}, [{ program: PROGRAM, built_in: true }], PROGRAM, BOB, art)],
  [true, false, false, false]);
check("itemDescription: only when the record carries one", [itemDescription(art), itemDescription({ description: "  A harbour at dawn. " }), itemDescription(null)], ["", "A harbour at dawn.", ""]);
check("itemFields: the four, then every other field by its name", itemFields({ ...art, edition_note: "first run", extra: null, nested: { a: 1 } }).map((r) => r.label + "=" + r.value),
  ["Supply=3", "Standard=Simple", "URI=ipfs://bafyart/meta", "Creators=me", "Edition note=first run", "Extra=none given"]);
check("itemFields: unreadable", itemFields(null).length, 1);
check("editionLines", [editionLines({ state: "master", print_balance: "3", collection: art }), editionLines({ state: "copy", collection: { printable_supply: "2" } }), editionLines({ state: "master", print_balance: "1", collection: { printable_supply: "1" } }), editionLines({ state: "unreadable", unreadable: "it ends too early" })],
  [["Master · can print 2 more copies", "The collection allows the master and up to 2 copies.", "Copies of one collection are identical: there is no serial number."],
   ["Copy", "The collection allows the master and up to 1 copy.", "Copies of one collection are identical: there is no serial number."],
   ["Master · no prints left", "The collection has one NFT: the master, and no copies.", "Copies of one collection are identical: there is no serial number."],
   ["Its state can't be read (it ends too early)."]]);
check("editionLines: a supply too big for a number is left out", editionLines({ state: "copy", collection: { printable_supply: "18446744073709551615" } }).length, 2);
check("registryLine", [registryLine(null), registryLine({ registry: false }), registryLine({ registry: true, verified: true }), registryLine({ registry: true, verified: true, stale: true }), registryLine({ registry: true, verified: false, kind: "unregistered" }), registryLine({ registry: true, verified: false })],
  ["The Program Registry isn't installed: its source is unknown.", "The Program Registry isn't installed: its source is unknown.", "Source verified (Program Registry)", "Changed: it runs other code than was registered.", "Not in the Program Registry: its source is unknown.", "Source not verified (Program Registry)"]);
check("ownerShown: the name, else the short account", [ownerShown("Public/" + BOB, "alice"), ownerShown("Public/" + BOB, ""), ownerShown("Private/" + BOB, undefined)], ["alice", "Public HLEwXN…eim2", "Private HLEwXN…eim2"]);
check("toName: lns: and @ written names", [toName("lns:Alice"), toName("@bob.lez")], ["alice", "bob"]);
check("toName: names only", [toName("Alice.lez"), toName(" bob "), toName("Public/" + BOB), toName("b@d"), toName(""), toName("a".repeat(64) + ":" + "b".repeat(VPK_HEX))], ["alice", "bob", "", "", "", ""]);
const NOW = 1800000000000;
check("nameStep: an account", nameStep("alice", "alice", { account: BOB, never: true, expires: null }, NOW), { state: "ok", name: "alice", account: BOB, via: "" });
check("nameStep: Public/ prefix dropped", nameStep("alice", "alice", { account: "Public/" + BOB, never: true }, NOW).account, BOB);
check("nameStep: nobody holds it", nameStep("alice", "alice", null, NOW), { state: "not_registered", name: "alice", account: "", via: "" });
check("nameStep: past its grace period counts as not registered (as the names tool does)", [nameStep("old", "old", { account: BOB, never: false, expires: NOW - GRACE_MS }, NOW).state, nameStep("old", "old", { account: BOB, never: false, expires: NOW - GRACE_MS + 1 }, NOW).state], ["not_registered", "ok"]);
check("nameStep: no public account", nameStep("jef", "jef", { account: null, npk: "ab", never: true }, NOW), { state: "no_account", name: "jef", account: "", via: "" });
check("nameStep: one forward followed, then no more", [nameStep("old", "old", { account: BOB, forward: "new", never: true }, NOW), nameStep("old", "new", { account: ART, forward: "newer", never: true }, NOW), nameStep("old", "new", null, NOW)],
  [{ follow: "new" }, { state: "ok", name: "new", account: ART, via: "old" }, { state: "not_registered", name: "new", account: "", via: "old" }]);
check("namePreview: none for no name", namePreview("", null), null);
check("namePreview: checking, and never another name's answer", [namePreview("alice", null).kind, namePreview("alice", { typed: "alic", state: "ok", name: "alic", account: BOB }).kind, namePreview("alice", { typed: "alice", state: "checking" }).text], ["working", "working", "Checking alice…"]);
check("namePreview: the answer", [
  namePreview("alice", { typed: "alice", state: "ok", name: "alice", account: BOB, via: "" }),
  namePreview("old", { typed: "old", state: "ok", name: "new", account: BOB, via: "old" }).text,
  namePreview("alice", { typed: "alice", state: "not_registered", name: "alice", via: "" }).text,
  namePreview("old", { typed: "old", state: "not_registered", name: "new", via: "old" }).text,
  namePreview("jef", { typed: "jef", state: "no_account", name: "jef", via: "" }).text,
  namePreview("jef", { typed: "jef", state: "unavailable", why: "the names tool isn't set up" }).text],
  [{ text: "Sends to alice · HLEwXN…eim2", kind: "done" }, "Sends to new · HLEwXN…eim2 (old points to new)", "Nobody holds alice. Check the spelling.", "old points to new, which nobody holds.",
   "jef has no public account, so it can't take an NFT by name. Ask for an account (Public/…) or receiving keys.", "Couldn't check jef: the names tool isn't set up. Try again in a moment."]);
const keysTyped = "a".repeat(64) + ":" + "b".repeat(VPK_HEX);
check("itemSendWhy", [
  itemSendWhy("", null, false), itemSendWhy("b@d", null, false), itemSendWhy("Public/" + BOB, null, false), itemSendWhy(keysTyped, null, false), itemSendWhy(keysTyped, null, true),
  itemSendWhy("alice", null, false), itemSendWhy("alice", { typed: "alice", state: "ok", name: "alice", account: BOB }, false), itemSendWhy("jef", { typed: "jef", state: "no_account", name: "jef" }, false)],
  ["Enter where it goes first.", "That isn't an LNS name, an account or receiving keys.", "", "Tick the box above to send to receiving keys.", "", "Checking the name…", "", "This name can't take it: see above."]);

// Basecamp's QML engine has no BigInt and reads some words as reserved: nft.mjs uses neither.
const code = source.split("\n").filter((l) => !l.trim().startsWith("//")).join("\n");
check("nft.mjs: no BigInt literals or calls", /\b\d+n\b|\bBigInt\s*\(/.test(code), false);
check("nft.mjs: no variables named like Java's reserved words", /\b(?:const|let|var|function)\s+(?:short|int|long|byte|char|float|double|boolean|native|package|private|protected|public|static|synchronized|throws|transient|volatile|goto|abstract|final|implements|interface)\b/.test(code), false);

// ── Phase 2 NFT viewing: Logos Storage media, privacy consent, media states, collection grid ──
const STORAGE_CID = "zDvZRwzm78buYYJGskHsMFmAQ9PbhZRRg8DgLUMhiGoUAqKewsiC";
const STORAGE_HASH = "a1b2c3d4".repeat(8);
const STORAGE_URI = "logos:" + STORAGE_CID + "#sha256=" + STORAGE_HASH;

check("privacyLabel", [privacyLabel(false), privacyLabel(true)], ["Public", "Private"]);
check("parseMediaUri: valid Logos Storage URI normalizes hash", parseMediaUri("logos:" + STORAGE_CID + "#sha256=" + STORAGE_HASH.toUpperCase()),
  { kind: "storage", state: "opt_in", cid: STORAGE_CID, sha256: STORAGE_HASH });
check("parseMediaUri: max 256 UTF-8 bytes enforced", [
  parseMediaUri("logos:" + "z".repeat(MAX_MEDIA_URI - "logos:".length - "#sha256=".length - 64) + "#sha256=" + STORAGE_HASH).kind,
  parseMediaUri("logos:" + "z".repeat(MAX_MEDIA_URI - "logos:".length - "#sha256=".length - 63) + "#sha256=" + STORAGE_HASH).state,
  parseMediaUri("https://img.example.com/" + "é".repeat(120)).state,
], ["storage", "unverified", "unverified"]);
check("parseMediaUri: missing/bad sha256 is unverified, empty is missing", [
  parseMediaUri("").state,
  parseMediaUri("logos:" + STORAGE_CID).state,
  parseMediaUri("logos:" + STORAGE_CID + "#sha256=deadbeef").state,
  parseMediaUri("logos:short#sha256=" + STORAGE_HASH).state,
  parseMediaUri("ipfs://bafy").state,
], ["missing", "unverified", "unverified", "unverified", "unverified"]);

check("imageConsent: Logos Storage is opt-in, names node and network disclosure, checks SHA-256",
  ((c) => [c.allowed, c.kind, c.secondAsk, c.text.includes("127.0.0.1:9090"), c.text.includes("Logos Storage network"), c.text.includes("SHA-256")])(imageConsent(STORAGE_URI, false, "http://127.0.0.1:9090")),
  [true, "storage", false, true, true, true]);
check("imageConsent: private-only requires second ask for both https and Logos Storage",
  [imageConsent("https://img.example.com/a.png", true).secondAsk, imageConsent(STORAGE_URI, true).secondAsk, imageConsent("https://img.example.com/a.png", false).secondAsk, imageConsent(STORAGE_URI, false).secondAsk],
  [true, true, false, false]);
check("imageConsent: unverified logos URI is refused", [imageConsent("logos:" + STORAGE_CID, false).allowed, imageConsent("logos:" + STORAGE_CID, false).unverified], [false, true]);

check("mediaState: missing, unverified, not_loaded, loading, failed, hash-unverified, ready", [
  mediaState(null, null).state,
  mediaState({ uri: "" }, null).state,
  mediaState({ uri: "logos:" + STORAGE_CID }, null).state,
  mediaState({ uri: STORAGE_URI }, null).state,
  mediaState({ uri: STORAGE_URI }, { state: "loading" }).state,
  mediaState({ uri: STORAGE_URI }, { state: "failed", error: "Logos Storage couldn't be reached." }).state,
  mediaState({ uri: STORAGE_URI }, { state: "unverified", error: "SHA-256 mismatch." }).state,
  mediaState({ uri: STORAGE_URI }, { state: "ready", pixels: { width: 2, height: 2, pixels: "AAAA" } }).state,
], ["missing", "missing", "unverified", "not_loaded", "loading", "failed", "unverified", "ready"]);
check("errorIsUnverified", [errorIsUnverified("didn't match its SHA-256 hash (unverified)"), errorIsUnverified("couldn't reach Logos Storage")], [true, false]);

check("storageNodeProblem: loopback http only", [
  storageNodeProblem(""),
  storageNodeProblem(DEFAULT_STORAGE_URL),
  storageNodeProblem("HTTP://127.0.0.1:8080"),
  storageNodeProblem("http://localhost:9090/"),
  storageNodeProblem("http://[::1]:8080"),
  storageNodeProblem("http://192.168.1.5:8080") !== "",
  storageNodeProblem("http://127.00.0.1:8080") !== "",
  storageNodeProblem("https://127.0.0.1:8080") !== "",
  storageNodeProblem("http://127.0.0.1:0") !== "",
  storageNodeProblem("http://127.0.0.1:8080/path") !== "",
], ["", "", "", "", "", true, true, true, true, true]);

const cards = collectionCards(programs);
check("collectionCards: art metadata, count, Public/Private breakdown, firstAccount",
  cards.map((c) => [c.title, c.count, c.publicCount, c.privateCount, c.isPrivateOnly, c.countLabel, c.firstAccount]),
  [["Art", 2, 1, 1, false, "2 held · 1 Public, 1 Private", "Public/" + BOB]]);

// UI guardrails for Phase 2 NFT viewing: Public/Private labels, second ask tick, no Make private/public actions yet.
check("Main.qml Collectibles: no Make private / Make public actions added yet", /\bMake private\b|\bMake public\b/i.test(block), false);
check("Main.qml Collectibles: second ask tick for private-only media", block.includes("nftConsentSecondTicked"), true);
check("Main.qml Collectibles: Public / Private badge and collection grid present", [block.includes("NftPrivacyBadge"), block.includes("NftArt"), block.includes("nftCollectionCards")].every(Boolean), true);

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures === 0 ? 0 : 1);
