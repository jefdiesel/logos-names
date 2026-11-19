// SPDX-License-Identifier: MIT OR Apache-2.0
// Checks the release-relay fee logic (qml/releasefee.mjs). Run with: node basecamp/releasefee.test.mjs
import * as F from "./qml/releasefee.mjs";

let failures = 0;
const check = (label, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) failures++;
  console.log((ok ? "ok   " : "FAIL ") + label + (ok ? "" : `\n   got  ${JSON.stringify(got)}\n   want ${JSON.stringify(want)}`));
};

const features = ["private_send_back", "release_fee_choice"];
// The shape the bridge's send_back_fee_menu answers (sendback.rs fee_menu_view).
const reply = {
  symbol: "ETH", denom_text: "0.1 ETH",
  choices: [
    { choice: 0, fee: "0", title: "No relay fee", text: "You finish the release on Ethereum yourself, or the bridge's settlement service finishes it for nothing. A paid release relayer can't take it.", receive_text: "0.1 ETH", default: true },
    { choice: 1, fee: "500000000000000", title: "Up to 0.0005 ETH (0.50%)", text: "Anyone who runs a release relayer may finish this release for you and keep up to this much; you get the rest.", receive_text: "At least 0.0995 ETH", default: false },
    { choice: 2, fee: "1000000000000000", title: "Up to 0.001 ETH (1.00%)", text: "…", receive_text: "At least 0.099 ETH", default: false }
  ],
  who: "The relay fee goes to whoever sends the release to the Ethereum vault, named in the release proof: a release relayer, or nobody if you send it yourself. The bridge keeps none of it.",
  privacy: "Pick the fee most people pick: a release with an unusual fee is easier to tell apart from the others of the same amount."
};

// The sheet.
check("no feature: off, with the old-bridge note", [F.releaseFeeSheet(["private_send_back"], reply, 1).state, F.releaseFeeSheet([], reply, 1).note], ["off", F.RELEASE_FEE_OLD_BRIDGE]);
check("being read: says so at once", [F.releaseFeeSheet(features, null, 0).state, F.releaseFeeSheet(features, null, 0).note], ["reading", "Reading the fee choices…"]);
check("failed or empty: no fee, says it can still go", [F.releaseFeeSheet(features, { failed: true }, 0).state, F.releaseFeeSheet(features, { choices: [] }, 0).note], ["failed", F.RELEASE_FEE_FAILED]);
const s0 = F.releaseFeeSheet(features, reply, undefined);
check("default is no relay fee", [s0.state, s0.chosen, s0.rows.map(r => r.selected)], ["ready", 0, [true, false, false]]);
check("rows keep the bridge's words and amounts", s0.rows.map(r => [r.choice, r.title, r.receive]), [[0, "No relay fee", "0.1 ETH"], [1, "Up to 0.0005 ETH (0.50%)", "At least 0.0995 ETH"], [2, "Up to 0.001 ETH (1.00%)", "At least 0.099 ETH"]]);
check("who earns it and the privacy note are shown", [/release relayer/.test(s0.who), /most people/.test(s0.privacy)], [true, true]);
check("a pick is kept", F.releaseFeeSheet(features, reply, 1).rows.map(r => r.selected), [false, true, false]);
check("an unknown pick falls back to no fee", F.releaseFeeSheet(features, reply, 7).chosen, 0);

// The confirm line.
check("line, no fee", F.releaseFeeLine(s0), F.RELEASE_FEE_NONE_LINE);
check("line, a fee", F.releaseFeeLine(F.releaseFeeSheet(features, reply, 1)), "Relay fee: up to 0.0005 ETH (0.50%). A release relayer that finishes it may keep that much; you get the rest.");
check("line, nothing read: no fee", F.releaseFeeLine(F.releaseFeeSheet(features, null, 1)), F.RELEASE_FEE_NONE_LINE);

// The call.
const args = ["ETH", "1", "Private/AAA", "logoswd1xyz"];
check("with the feature, choice 1", F.releaseFeePlanCall(features, args, 1), { method: "send_back_plan_with", args: args.concat(['{"relay_fee_choice":1}']) });
check("with the feature, choice 0 still goes through _with (beats the code's own)", F.releaseFeePlanCall(features, args, 0), { method: "send_back_plan_with", args: args.concat(['{"relay_fee_choice":0}']) });
check("old bridge, no fee: the old call, unchanged", F.releaseFeePlanCall(["private_send_back"], args, 0), { method: "send_back_plan", args: args });
check("old bridge, a fee: refused, never dropped", !!F.releaseFeePlanCall(["private_send_back"], args, 1).error, true);
check("not a choice: refused", !!F.releaseFeePlanCall(features, args, -1).error, true);
check("args are strings", F.releaseFeePlanCall(features, ["ETH", 1, "Private/AAA", "c"], 0).args.slice(0, 4), ["ETH", "1", "Private/AAA", "c"]);

// Words.
const all = [F.RELEASE_FEE_HEADING, F.RELEASE_FEE_READING, F.RELEASE_FEE_FAILED, F.RELEASE_FEE_OLD_BRIDGE, F.RELEASE_FEE_NONE_LINE].join(" ");
check("no forbidden words", /official|lookup/i.test(all), false);

console.log(failures ? `${failures} FAILED` : "ALL PASS");
process.exit(failures ? 1 : 0);
