#!/usr/bin/env python3
"""Runs every change the `names` module can make against a live registry.

It drives the module the way an app does: send one change, then read the chain
until the change shows, and only then send the next. Needs a logosctl session
with `names` and `lez_core` loaded, a wallet open in `lez_core` that holds two
funded accounts, and the registry already chosen with `set_registry`.

Usage: try-changes.py <logosctl binary> <session dir> <account A> <account B>
"""
import json, subprocess, sys, time

LOGOSCTL, SESSION, A, B = sys.argv[1:5]
NAME = "trial-%d" % (time.time() % 100000)
failures = 0


def call(method, *args):
    """One module call; returns the value, or raises the module's error."""
    typed = ["str:" + arg if isinstance(arg, str) else str(arg) for arg in args]
    out = subprocess.run([LOGOSCTL, "--config-dir", SESSION, "call", "names", method, *typed],
                         capture_output=True, text=True).stdout
    reply = json.loads(out)["result"]
    if isinstance(reply, str):
        reply = json.loads(reply)
    if not reply["success"]:
        raise RuntimeError(reply["error"])
    return reply["value"]


def change(label, method, args, settled):
    """Sends a change and waits until `settled()` reads it back from the chain."""
    global failures
    started = time.time()
    try:
        call(method, *args)
        while not settled():
            if time.time() - started > 60:
                raise RuntimeError("sent, but it never showed on the chain")
            time.sleep(2)
        print("ok   %-34s %4.0fs" % (label, time.time() - started))
    except Exception as error:
        failures += 1
        print("FAIL %-34s %s" % (label, error))


def refused(label, method, args, expect):
    """A change the module must refuse before sending anything."""
    global failures
    try:
        call(method, *args)
        failures += 1
        print("FAIL %-34s was not refused" % label)
    except RuntimeError as error:
        ok = expect in str(error)
        failures += 0 if ok else 1
        print("%s %-34s %s" % ("ok  " if ok else "FAIL", label, error))


record = lambda: call("resolve", NAME)
listed = lambda: [entry["name"] for entry in call("list", 100, "")["names"]]
page = "<h1>%s</h1>\n<p>Written through the module.</p>" % NAME

print("trying", NAME + ".lez")
change("register", "register", [NAME + ".lez", A], lambda: (record() or {}).get("owner") == A)
refused("register a taken name", "register", [NAME, B], "already registered")
newest = listed()[0] == NAME
failures += 0 if newest else 1
print("%s it is the newest name in the list" % ("ok  " if newest else "FAIL"))
change("set where it pays", "set_account", [NAME, B], lambda: record()["account"] == B)
change("add an entry", "set_text", [NAME, "agent", "https://agents.example/" + NAME],
       lambda: record()["texts"].get("agent", "").endswith(NAME))
change("change the entry", "set_text", [NAME, "agent", "changed"], lambda: record()["texts"].get("agent") == "changed")
change("add a second entry", "set_text", [NAME, "cid", "zb2rh"], lambda: record()["texts"].get("cid") == "zb2rh")
change("remove an entry", "set_text", [NAME, "agent", ""], lambda: "agent" not in record()["texts"])
refused("remove an entry that is not there", "set_text", [NAME, "agent", ""], "no entry")
refused("a bad entry key", "set_text", [NAME, "Bad Key", "x"], "key")
change("publish a page", "set_page", [NAME, "text/html; charset=utf-8", page],
       lambda: (call("page", NAME) or {}).get("text") == page)
change("show the account as the name", "set_primary", [A, NAME],
       lambda: (call("reverse", A) or {}).get("name") == NAME)
change("stop taking payments", "clear_addresses", [NAME], lambda: record()["account"] is None)
refused("a name that points elsewhere", "set_primary", [B, NAME], "does not point")
change("transfer to B", "transfer", [NAME, B], lambda: record()["owner"] == B)
after = record()
clean = after["texts"] == {} and after["account"] is None and call("page", NAME) is None
failures += 0 if clean else 1
print("%s transfer cleared entries and page" % ("ok  " if clean else "FAIL"))
gone = call("reverse", A) is None
failures += 0 if gone else 1
print("%s the old holder is no longer shown as it" % ("ok  " if gone else "FAIL"))
change("the new holder edits it", "set_text", [NAME, "chat", "ab" * 32], lambda: "chat" in record()["texts"])
change("delete", "release", [NAME], lambda: record() is None)
unlisted = NAME not in listed()
failures += 0 if unlisted else 1
print("%s it left the list" % ("ok  " if unlisted else "FAIL"))
refused("delete a name nobody holds", "release", [NAME], "not registered")
change("register it again, to B", "register", [NAME, B], lambda: (record() or {}).get("owner") == B)
change("delete it again", "release", [NAME], lambda: record() is None)
change("reset A's shown name", "set_primary", [A, ""], lambda: call("reverse", A) is None)

print("ALL PASS" if failures == 0 else "%d FAILED" % failures)
sys.exit(1 if failures else 0)
