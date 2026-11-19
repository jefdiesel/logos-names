# Marked QML patches (branch names-privacy-m5), not applied

`basecamp/qml/Main.qml`'s bridge screens are another session's file (branch `names-roadmap-2`), so these
hook-ups are kept as patches for the conductor to apply there. Each hunk is marked `PATCH privacy-m5 …` /
`END PATCH`. Both apply to this branch's `Main.qml` (`git apply --check`), together too, and with both
applied `node basecamp/logic.test.mjs` and `node basecamp/claimdelay.test.mjs` still pass (checked
2026-11-19 in a scratch worktree). Neither is rendered.

| Patch | What | Logic (tested) |
|---|---|---|
| `release-fee-main-qml.patch` | Bridge to Ethereum: the release-relay fee choices under the amount (no relay fee first, the default), the chosen one planned through `send_back_plan_with` | `qml/releasefee.mjs`, `releasefee.test.mjs` |
| `claim-time-card-main-qml.patch` | The home card's "Arriving from Ethereum" row: the planned claim time of a private receive | `qml/claimdelay.mjs` `claimDelayCardLine`, `claimdelay.test.mjs` |

Apply: `git apply basecamp/patches/<name>.patch` from the repository root.
