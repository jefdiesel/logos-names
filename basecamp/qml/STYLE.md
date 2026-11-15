# Names: one look (Main.qml)

Every page uses these parts. Don't draw a new status line, step row, back link, tick box or
choice by hand; use the component. `logic.test.mjs` checks the rules marked (test).

## Colours (tokens on `root`)

| token | what it is for, and nothing else |
|---|---|
| `brass` | the primary button's fill, focus rings, a chosen choice or account, a ticked box |
| `waitTone` (amber) | **waiting** status lines (and the one caution box below) |
| `alarm` (red) | **problem** status lines (test: no red borders, no red `Hint`) |
| `good` (green) | **done** status lines |
| `ink` / `dim` | text / secondary text and labels ("Private", "main", "Public") |
| `raised`, `card`, `line` | boxes and panels (neutral: the under-way panel too; no violet) |

`cautionTone` = `waitTone`, used only on the consent box of a call the wallet truly cannot decode,
marked "!" (not the clock).

## Status lines: `StatusLine { kind; text; actionLabel; onAct }`

Exactly four kinds; one mark, one colour, one size (`statusPx()` = 16), placed directly under the
control or step they belong to (never a banner above the page's content).

| kind | colour + mark | says | control |
|---|---|---|---|
| `working` | dim, three dots | "Checking…": this app is doing it | the button shows it ("Checking…") and is disabled |
| `waiting` | amber, clock | something outside takes time: how long, what happens next | a wait may offer "Check again" |
| `problem` | red, cross | what failed or was refused, in one sentence | exactly one next action, as a button |
| `done` | green, tick | what happened and what is now true ("Claimed in block 10616 · Now in the account: 0.01 ETH") | none |

`kind: ""` is a plain note (ink, no mark). Which kind: `Logic.noticeKind(message, how)` (busy, no
answer, starting, 429 are waits, never red), `Logic.toneKind(tone)` for the bridge's progress,
`Logic.setupKind(state)` for the bridge setup box, `Logic.claimNext(code, …)` for a claim refusal
(its kind and its one action). `say(message, how)` takes `true` (failure), `false` (done) or a
kind; `noticeIsError` still means "it failed" (an app's request is answered with it).

## Steps: `StepHeader { steps }`

One pill row + "Step n of N · title", the same on Claim, Receive from Ethereum and Bridge to
Ethereum. Done pills: ink, "✓ word". Current: outlined in ink, bold. Later: dim. The title of
a current step never carries a tick; a done flow's last step is "Claimed" / "Received on
Ethereum" and the done status line says the rest.

## Page header: `PageHeader { title; net; onBack }`

A real back button (`BackButton`: bordered, 17 px, 36 px tall, hover and pressed fills) at the
same place on every page, the title at 28 px, then the network line. Every page below the wallet
is `Layout.maximumWidth: root.pageWidth` (680). (test: no "‹ Wallet" / "← Back" links)

## Buttons

- `Action { strong: true }`: the one primary on a screen (brass fill). Secondary: `Action`.
  Small things beside them: `Link`.
- A disabled button has its reason in a `WhyLine` under it ("Tick the box above to approve.",
  `Logic.approveWhy`, `Logic.receiveMakeWhy`).
- Choices (a token, an amount, an account) are `Choice` chips (brass outline when chosen), never
  filled buttons.
- No red buttons: a step that can't be undone says so in its confirm line.
- Approvals: "Approve" and "Reject" (never "Not now": it rejects for good). After one ends
  without a yes: `Logic.approvalEndedLine(state)` + "Start again"; it never says who said no.

## Tick boxes: `Tick { checked; text; caution; onToggled }`

An ordinary box with plain words. `caution: true` (amber border and "!") only when the wallet
cannot decode the call (`appSheetCaution`). Never red; never coloured before it is ticked.

## Times

One place per number: send-back `Logic.SEND_BACK_TIMES` (burn about 15 s, release proof about
20 s, wrapper about 3 min once run) and `Logic.SEND_BACK_BY_HAND` (what the bridge's operator does
by hand today), said once under the panel.
