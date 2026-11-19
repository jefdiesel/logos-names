// SPDX-License-Identifier: MIT OR Apache-2.0
// Which relay sent a claim or a send-back's burn, from the bridge module's `relay` answer
// ({url, label, own, payer} on relayed_claim_status and send_back_status; rfp-022
// docs/privacy-m5/open-relay.md). The QML imports this as `import "relayline.mjs" as RelayLine`;
// its Node test is basecamp/relayline.test.mjs.

/// The relay's name for people, only for a relay other than the bridge's own: "" for the bridge's
/// own relay or an answer without `relay` (an older module), so those screens read as before.
export function relayUsed(relay) {
  if (!relay || typeof relay !== "object" || relay.own !== false) return "";
  return String(relay.label || "").replace(/\s+/g, " ").trim().slice(0, 60);
}

/// The words after a claim's result: " Sent by Relay B." or "".
export function relayUsedLine(relay) {
  const name = relayUsed(relay);
  return name ? " Sent by " + name + "." : "";
}
