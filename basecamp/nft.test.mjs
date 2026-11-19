// Checks Collectibles' rules (qml/nft.mjs). Run with: node basecamp/nft.test.mjs
import { privateOnly, accountShown, idShown, tokenLine, collectionTitle, metadataRows, accountGroups, collectionCounts, receiveChoices, collectionLink, linkOf, toProblem, toTheirKeys, approveWhy, defaultPayer, jobLine, jobEnded, imageConsent, burnConfirm } from "./qml/nft.mjs";
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

check("toProblem good", ["alice", "alice.lez", "Public/" + BOB, "Private/" + BOB, "ab".repeat(32) + ":" + "cd".repeat(10)].map(toProblem), ["", "", "", "", ""]);
check("toProblem bad", ["", BOB, "Public/x!", "ab:cd", "two words"].map((t) => toProblem(t) !== ""), [true, true, true, true, true]);
check("toTheirKeys", [toTheirKeys("ab".repeat(32) + ":cd"), toTheirKeys("alice")], [true, false]);

// The sheet's button waits for each needed choice, and says which.
const sheet = { action: "send", proven: true, payers: [{ account: "Private/A", enough: false }, { account: "Private/B", enough: true }], warnings: ["x"] };
check("approveWhy no sheet", approveWhy(null, {}), "Checking what this does…");
check("approveWhy payer", approveWhy(sheet, {}), "Choose the private account that pays the fee.");
check("approveWhy poor payer", approveWhy(sheet, { payer: "Private/A" }), "That account can't cover the fee reserve; choose another.");
check("approveWhy warnings", approveWhy(sheet, { payer: "Private/B" }), "Tick the box above to go ahead.");
check("approveWhy ready", approveWhy(sheet, { payer: "Private/B", warnings: true }), "");
check("approveWhy burn", approveWhy({ action: "burn", proven: false, warnings: [] }, {}), "Tick that you understand it can't be undone.");
check("approveWhy refused shows the refusal instead", approveWhy({ refusal: "That account already holds an NFT of this collection." }, {}), "");
check("defaultPayer", [defaultPayer(sheet, "Private/A"), defaultPayer(sheet, "Private/B"), defaultPayer({ payers: [] }, "")], ["Private/B", "Private/B", ""]);

check("jobLine public", jobLine({ label: "Sending Art", proves: false, started: 0 }, 12000), "Sending Art: sending it publicly, one block (about 30 seconds), 12s so far.");
check("jobLine proven", jobLine({ label: "Sending Art", proves: true, started: 0 }, 180000).includes("7-12 minutes"), true);
check("jobEnded done", jobEnded({ label: "Sending Art", kind: "nft_send" }, { state: "done", block: 7 }), { kind: "done", text: "Sending Art: done, in block 7." });
check("jobEnded refused", jobEnded({ label: "Sending Art", kind: "nft_send" }, { state: "failed", refusal: "That account isn't ready for this collection: it has to receive a holding first." }).kind, "problem");
check("jobEnded receive", [jobEnded({ kind: "nft_receive" }, { state: "done", nothing: true }).text, jobEnded({ kind: "nft_receive" }, { state: "done", received: 1 }).text], ["There was nothing to receive.", "Received 1 NFT."]);

check("privateOnly", [privateOnly(programs, PROGRAM, ART), privateOnly([{ program: PROGRAM, holdings: [programs[0].holdings[1]] }], PROGRAM, ART)], [false, true]);
check("imageConsent non-https", imageConsent("ipfs://bafy", false).allowed, false);
check("imageConsent https names the host", imageConsent("https://img.example.com/a.png", true).text.includes("img.example.com"), true);
check("imageConsent private warns", imageConsent("https://img.example.com/a.png", true).text.includes("Only private accounts"), true);
check("burnConfirm", [burnConfirm({ state: "copy" }).includes("this copy"), burnConfirm({ state: "master" }).includes("master")], [true, true]);

// The words the wallet never uses, in the new code and the Collectibles block of Main.qml.
const source = readFileSync(new URL("./qml/nft.mjs", import.meta.url), "utf8");
const main = readFileSync(new URL("./qml/Main.qml", import.meta.url), "utf8");
const block = main.slice(main.indexOf("// ── Collectibles ──"), main.indexOf("// ── end Collectibles\n")) + main.slice(main.indexOf("// ── Collectibles page"), main.indexOf("// ── end Collectibles page"));
check("Collectibles block present", block.length > 1000, true);
for (const [label, text] of [["nft.mjs", source.split("\n").filter((l) => !l.startsWith("//")).join("\n")], ["Main.qml Collectibles", block]]) {
  check(label + ": no 'lookup' / 'official'", /\blook ?up\b|\bofficial\b/i.test(text), false);
}
// No polling while the page is closed: every NFT timer runs only while Collectibles is open.
const timers = block.split("Timer {").slice(1).map((t) => t.slice(0, 400)).filter((t) => /repeat: true/.test(t));
check("every Collectibles timer needs the page open", timers.length > 0 && timers.every((t) => t.includes('root.section === "collectibles"')), true);
check("every Collectibles timer is 30 s or slower", timers.every((t) => Number((/interval: (\d+)/.exec(t) || [0, 0])[1]) >= 30000), true);

console.log(failures === 0 ? "ALL PASS" : failures + " FAILED");
process.exit(failures === 0 ? 0 : 1);
