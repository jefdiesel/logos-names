#!/usr/bin/env python3
"""Renders the real Names app (qml/Main.qml) with REAL answers from the names module.

render.py answers the app from stand-ins. This answers it from a running names module
instead: every call the app makes is recorded, asked of the module over logosctl (real
Logos Core IPC, so the module's real config, CLI, wallet and chain), and the app is
rendered again with the answers, until it asks nothing new. What it draws is what
Basecamp would show with the same module and setup.

  replay.py <qml binary> <out dir> <logosctl> <session config dir> <scenario>...

The session (LOGOSCTL_CONFIG_DIR, HOME) must already run with `names` loaded. Nothing
is sent: a method that writes is never asked (see WRITES); the app gets a refusal.
Modules other than names (chat_module, lez_core) answer as nothing is running.
"""
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))

# Calls that change something. Never asked of the module, whatever the app does.
WRITES = {
    "cli_pay", "cli_pay_account", "cli_register", "cli_set_text", "cli_profile", "cli_set_page", "cli_shield",
    "cli_faucet", "cli_bridge", "cli_token_send", "cli_token_receive", "cli_account_new", "cli_account_label",
    "cli_account_use", "cli_create_wallet", "cli_restore_wallet", "cli_install", "cli_claim", "chain_claim",
    "chain_send", "chain_split", "chain_bridge", "cli_bridge", "cli_bridge_to",
    "set_main_name", "set_token_program", "watch_token", "board_post", "board_toggle_like", "board_follow",
    "make_public", "register_name", "set_account", "set_text", "set_profile", "set_page", "transfer", "release",
    "keep", "approve_request", "deny_request", "request_access", "propose_transfer", "receive_privately",
}

SCENARIOS = {
    "home": "",
    "home-totals": "Qt.callLater(function() { root.walletSetAll(false) })",
    "receive": 'root.openReceive({ kind: "lez" })',
    "names": 'root.openWallet("names")',
    "settings": 'root.section = "settings"; root.loadAccounts()',
    "account-low": 'root.openAccount(root.walletGroups[0].rows.filter(function(r) { return !r.canSend && r.balance !== "0" })[0])',
    "activity": 'root.section = "activity"; root.loadActivity()',
    "profile": "root.openProfile()",
    "search-jef": 'root.go("search"); root.show("jef")',
    "send": 'Qt.callLater(function() { var rows = root.walletRows.filter(function(r) { return r.kind === "lez" }); if (rows.length) root.openSend(rows[0], "jef") })',
}

BRIDGE = r"""
    width: 1180; height: %(height)s
    readonly property var replayed: (%(answers)s)
    property var logos: QtObject {
        function callModuleAsync(module, method, args, callback, timeout) {
            var reply = callModule(module, method, args)
            Qt.callLater(function() { callback(reply) })
        }
        function callModule(module, method, args) {
            var key = module + "|" + method + "|" + JSON.stringify(args)
            if (root.replayed[key] !== undefined)
                return root.replayed[key]
            console.log("REPLAY-MISS " + key)
            if (module !== "names")
                return module === "chat_module" ? JSON.stringify(null) : ""
            return JSON.stringify({ success: false, value: null, error: "(not asked yet)" })
        }
        function request(what, args, callback) {}
    }
    Timer { interval: 600; running: true; onTriggered: { %(scenario)s; shot.start() } }
    Timer { id: shot; interval: 900; onTriggered: root.grabToImage(function(result) { result.saveToFile("%(out)s"); Qt.quit() }) }
"""


def logosctl_arg(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    return "str:" + str(value)


def ask(ctl, method, args):
    """The names module's own reply text for `method(args)`, over logosctl."""
    if method in WRITES:
        return json.dumps({"success": False, "value": None, "error": "replay: writes are never sent"})
    result = subprocess.run([ctl, "--json", "call", "names", method, *map(logosctl_arg, args)], capture_output=True, text=True, timeout=300)
    try:
        answer = json.loads(result.stdout)
    except ValueError:
        return json.dumps({"success": False, "value": None, "error": "logosctl: " + (result.stderr or result.stdout)[-200:]})
    reply = answer.get("result")
    if answer.get("status") != "ok":
        return json.dumps({"success": False, "value": None, "error": answer.get("message") or "the call failed"})
    return reply if isinstance(reply, str) else json.dumps(reply)


def render(qml, scenario, answers, out):
    source = open(os.path.join(HERE, "..", "qml", "Main.qml")).read()
    marker = "Item {\n    id: root\n"
    bridge = BRIDGE % {"answers": json.dumps(answers), "scenario": SCENARIOS[scenario], "out": out,
                       "height": 2400 if scenario in ("settings", "profile", "search-jef") else 1500}
    with tempfile.NamedTemporaryFile("w", suffix=".qml", delete=False, dir=os.path.join(HERE, "..", "qml")) as handle:
        handle.write(source.replace(marker, marker + bridge, 1))
    try:
        result = subprocess.run([qml, handle.name], capture_output=True, text=True, timeout=120)
    except subprocess.TimeoutExpired as hung:
        stderr = hung.stderr.decode() if isinstance(hung.stderr, bytes) else (hung.stderr or "")
        return [], ["RENDER HUNG (no picture taken): " + stderr[-300:]]
    finally:
        os.unlink(handle.name)  # never beside Main.qml for longer than a render
    misses, notes = [], []
    for line in result.stderr.splitlines():
        if "REPLAY-MISS " in line:
            misses.append(line.split("REPLAY-MISS ", 1)[1])
        elif "qml: names_ui:" in line:
            notes.append(line.split("qml: ", 1)[1])
    return misses, notes


def main():
    qml, out_dir, ctl = sys.argv[1], sys.argv[2], sys.argv[3]
    wanted = sys.argv[5:] or list(SCENARIOS)
    os.makedirs(out_dir, exist_ok=True)
    answers, asked = {}, []
    for scenario in wanted:
        out = os.path.join(out_dir, scenario + ".png")
        for round_ in range(8):
            misses, notes = render(qml, scenario, answers, out)
            fresh = [m for m in dict.fromkeys(misses) if m not in answers]
            if not fresh:
                break
            for key in fresh:
                module, method, args = key.split("|", 2)
                if module != "names":
                    answers[key] = "" if module != "chat_module" else json.dumps(None)
                    continue
                answers[key] = ask(ctl, method, json.loads(args))
                asked.append((method, json.loads(args), answers[key][:160]))
        print(f"{scenario}: rendered after {round_ + 1} rounds", *[f"  {n}" for n in dict.fromkeys(notes)][:8], sep="\n")
    with open(os.path.join(out_dir, "asked.json"), "w") as handle:
        json.dump(asked, handle, indent=1)
    print(f"{len(asked)} real calls asked of the names module; see asked.json")


main()
