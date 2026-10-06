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
mine = lambda: [entry["name"] for entry in call("mine")["names"]]
page = "<h1>%s</h1>\n<p>Written through the module.</p>" % NAME


def check(label, ok):
    global failures
    failures += 0 if ok else 1
    print("%s %s" % ("ok  " if ok else "FAIL", label))


YEAR, MONTH = 31_536_000_000, 2_592_000_000
print("trying", NAME + ".lez")
prices = call("prices")
check("the registry says what each term costs", [term["days"] for term in prices["terms"]] == [30, 365, 1825])
started = int(time.time() * 1000)
change("register for a year, paid privately", "register", [NAME + ".lez", A, "auto", 1],
       lambda: (record() or {}).get("owner") == A)
check("it is held for a year from now", abs(record()["expires"] - (started + YEAR)) < 300_000 and not record()["lapsed"])
until = record()["expires"]
change("renew for 30 days", "renew", [NAME, A, 0], lambda: record()["expires"] > until)
check("renewing added exactly 30 days", record()["expires"] == until + MONTH)
refused("a term that does not exist", "renew", [NAME, A, 7], "term")
check("it takes private payments from the start", record()["npk"] is not None and record()["account"] is None)
check("this device lists it among the wallet's names", NAME in mine())
refused("register a taken name", "register", [NAME, B, "", 1], "already registered")
change("add a public account", "set_account", [NAME, B], lambda: record()["account"] == B)
check("the private keys are still there", record()["npk"] is not None)
change("go back to private only", "receive_privately", [NAME, "auto"],
       lambda: record()["account"] is None and record()["npk"] is not None)
change("add an entry", "set_text", [NAME, "agent", "https://agents.example/" + NAME],
       lambda: record()["texts"].get("agent", "").endswith(NAME))
change("change the entry", "set_text", [NAME, "agent", "changed"], lambda: record()["texts"].get("agent") == "changed")
change("add a second entry", "set_text", [NAME, "cid", "zb2rh"], lambda: record()["texts"].get("cid") == "zb2rh")
change("remove an entry", "set_text", [NAME, "agent", ""], lambda: "agent" not in record()["texts"])
check("the other entry and the keys survived the edits", record()["texts"] == {"cid": "zb2rh"} and record()["npk"] is not None)
refused("remove an entry that is not there", "set_text", [NAME, "agent", ""], "no entry")
refused("a bad entry key", "set_text", [NAME, "Bad Key", "x"], "key")
change("publish a page", "set_page", [NAME, "text/html; charset=utf-8", page],
       lambda: (call("page", NAME) or {}).get("text") == page)
change("publish the account's name", "set_primary", [A, NAME],
       lambda: (call("reverse", A) or {}).get("name") == NAME)
refused("a name that points elsewhere", "set_primary", [B, NAME], "does not point")
change("stop taking payments", "clear_addresses", [NAME],
       lambda: record()["account"] is None and record()["npk"] is None)
change("transfer to B", "transfer", [NAME, B], lambda: record()["owner"] == B)
after = record()
check("transfer emptied the profile and the page",
      after["texts"] == {} and after["account"] is None and after["npk"] is None and call("page", NAME) is None)
check("the old holder is no longer shown as it", call("reverse", A) is None)
change("the new holder edits it", "set_text", [NAME, "chat", "ab" * 32], lambda: "chat" in record()["texts"])
change("delete", "release", [NAME], lambda: record() is None)
check("it is no longer among the wallet's names", NAME not in mine())
refused("delete a name nobody holds", "release", [NAME], "not registered")
change("register it again, to B, with nothing set", "register", [NAME, B, "", 0],
       lambda: (record() or {}).get("owner") == B)
check("nothing of the first registration came back", record()["npk"] is None and record()["texts"] == {})
check("and it is held for 30 days", abs(record()["expires"] - (int(time.time() * 1000) + MONTH)) < 300_000)
change("delete it again", "release", [NAME], lambda: record() is None)
change("withdraw A's published name", "set_primary", [A, ""], lambda: call("reverse", A) is None)

print("ALL PASS" if failures == 0 else "%d FAILED" % failures)
sys.exit(1 if failures else 0)
