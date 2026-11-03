// Drives the Testimonial app's own logic (apps/testimonial/src/qml/logic.mjs) against a live
// sequencer, doing what Main.qml does with the reads its backend makes.
//   node live.mjs <logic.mjs> <rpc> call <program> <submission> <Public/author> <username|-> <text>
//   node live.mjs <logic.mjs> <rpc> list <program> <submission>
const [logicPath, rpc, cmd, ...args] = process.argv.slice(2);
const L = await import(logicPath);

async function read(account, program) {
  const res = await fetch(rpc, { method: "POST", headers: { "Content-Type": "application/json" }, body: L.accountViewRequest(account, program) });
  const reply = await res.json();
  if (reply.error) throw new Error(JSON.stringify(reply.error));
  return L.actorState(reply.result, program);
}

if (cmd === "call") {
  const [program, submission, author, username, text] = args;
  const call0 = L.postCall(program, submission, username === "-" ? "" : username, text, 1, author);
  const existing = await read(call0.node, program);
  if (existing.length > 0) { console.log(JSON.stringify({ already: true, node: call0.node })); process.exit(0); }
  const clock = await read(L.CLOCK_ACCOUNT, L.CLOCK_PROGRAM);
  const chain = clock.length === 16 ? L.clockTime(clock) : 0;
  const now = L.postTime(chain, Date.now());
  const made = L.postCall(program, submission, username === "-" ? "" : username, text, now, author);
  console.log(JSON.stringify({ ...made, now, chain_time: chain, summary: L.postSummary(submission, author, text) }));
} else if (cmd === "list") {
  const [program, submission] = args;
  L.walk(program, submission, (a, cb) => read(a, program).then((b) => cb(null, b), (e) => cb(String(e))), null, (error, listing) => {
    if (error) { console.log(JSON.stringify({ error })); return; }
    const s = L.summarize(listing.items);
    console.log(JSON.stringify({ head: listing.head_address, count: listing.head && listing.head.count, summary: s,
      items: listing.items.map((t) => ({ seq: t.seq, author: t.author, username: t.username, text: t.text, at: L.formatTime(t.at), address: t.address })) }, null, 1));
  });
}
