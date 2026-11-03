// Asks a local faucet (chain/faucet.py on loopback) exactly as the Faucet app's backend does,
// and feeds each answer through the app's own Logic.interpret.
//   node faucet-live.mjs <logic.mjs> <faucet url> <account> [<account>…]
const [logicPath, url, ...accounts] = process.argv.slice(2);
const L = await import(logicPath);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function ask(account) {
  const res = await fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ account: L.bare(account) }), redirect: "manual" });
  let body = null;
  try { body = await res.json(); } catch (e) {}
  return { status: res.status, body, retry_after: res.headers.get("Retry-After") || "" };
}
const info = await fetch(url).then((r) => r.json().then((body) => ({ status: r.status, body })));
console.log("describe", JSON.stringify(L.describe(info)));
const grants = {};
for (const [i, account] of accounts.entries()) {
  const reply = await ask(account);
  const o = L.interpret(reply, Date.now(), grants[account] || 0);
  if (o.ok) grants[account] = Date.now();
  console.log(`ask ${i} ${L.short(account)} -> HTTP ${reply.status} ${JSON.stringify(reply.body)}\n   app says: ${o.message}${o.retryText ? " | ask again " + o.retryText : ""}${o.final ? " | final" : ""}`);
  if (i === 1) await sleep(25000); // let the first grant arrive, so the faucet is free again
}
