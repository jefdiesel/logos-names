#!/usr/bin/env python3
"""Runs every change the `names` module can make against a live registry.

It drives the module the way an app does: send one change, then read the chain
until the change shows, and only then send the next. Needs a logosctl session
with `names` and `lez_core` loaded, a wallet open in `lez_core` that holds two
public accounts each funded with more than a deposit, and a registry v5 in
deposit mode chosen with `set_registry`. Registering waits out the commitment
(about 15 minutes), twice.

Usage: try-changes.py <logosctl binary> <session dir> <account A> <account B>
"""
import json, subprocess, sys, time

LOGOSCTL, SESSION, A, B = sys.argv[1:5]
NAME = "trial-%d" % (time.time() % 100000)
failures = 0


def call(method, *args):
    """One module call; returns the value, or raises the module's error."""
    typed = ["str:" + arg if isinstance(arg, str) else str(arg).lower() for arg in args]
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


def holder():
    """The wallet account `mine` says holds the name: a record names only a tag."""
    return next((entry["owner"] for entry in call("mine")["names"] if entry["name"] == NAME), None)


def register(label, owner, private_account):
    """Registers in two steps: commit, wait until the commitment may be revealed
    (about 15 minutes), reveal, then wait for the name to show as `owner`'s."""
    global failures
    started = time.time()
    try:
        answer = call("register_name", NAME + ".lez", owner, private_account)
        if answer["stage"] != "committed":
            raise RuntimeError("the first step was %s, not a commitment" % answer["stage"])
        while True:
            answer = call("register_name", NAME, owner, private_account)
            if answer["stage"] in ("revealed", "registered"):
                break
            if time.time() - started > 40 * 60:
                raise RuntimeError("the commitment never became ready to reveal")
            time.sleep(30)
        while holder() != owner:
            if time.time() - started > 45 * 60:
                raise RuntimeError("revealed, but it never showed on the chain")
            time.sleep(5)
        print("ok   %-34s %4.0fs" % (label, time.time() - started))
    except Exception as error:
        failures += 1
        print("FAIL %-34s %s" % (label, error))


print("trying", NAME + ".lez")
settings = call("settings")
check("the registry takes deposits", settings is not None and settings["mode"] == "deposit")
quote = call("deposit", NAME)
check("a normal name takes the normal deposit", quote["tier"] == "normal" and quote["deposit"] == settings["deposits"]["normal"])
check("a two-character name is locked", call("deposit", "ab")["tier"] == "locked")
check("a three-character name takes the higher deposit", call("deposit", "qkz")["deposit"] == settings["deposits"]["three"])
register("register, paid privately", A, "auto")
check("it never expires and holds its deposit",
      record()["never"] and record()["expires"] is None and not record()["lapsed"] and record()["deposit"] == quote["deposit"])
check("it takes private payments from the start", record()["npk"] is not None and record()["account"] is None)
check("this device lists it among the wallet's names", NAME in mine())
refused("register a taken name", "register_name", [NAME, B, ""], "already registered")
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
change("set a profile in one go", "set_profile", [NAME, json.dumps({"display": "Trial", "bio": "hi", "cid": ""}), False],
       lambda: record()["texts"] == {"display": "Trial", "bio": "hi"})
change("replace the whole profile", "set_profile", [NAME, json.dumps({"theme": "dark"}), True],
       lambda: record()["texts"] == {"theme": "dark"} and record()["npk"] is not None)
change("publish a page", "set_page", [NAME, "text/html; charset=utf-8", page],
       lambda: (call("page", NAME) or {}).get("text") == page)
change("publish the account's name", "set_primary", [A, NAME],
       lambda: (call("reverse", A) or {}).get("name") == NAME)
refused("a name that points elsewhere", "set_primary", [B, NAME], "does not point")
change("stop taking payments", "clear_addresses", [NAME],
       lambda: record()["account"] is None and record()["npk"] is None)
change("transfer to B", "transfer", [NAME, B], lambda: holder() == B)
after = record()
check("transfer emptied the profile and the page, and kept the deposit",
      after["texts"] == {} and after["account"] is None and after["npk"] is None and call("page", NAME) is None
      and after["deposit"] == quote["deposit"])
check("the old holder is no longer shown as it", call("reverse", A) is None)
change("the new holder edits it", "set_text", [NAME, "chat", "ab" * 32], lambda: "chat" in record()["texts"])
change("release, refunding the deposit", "release", [NAME], lambda: record() is None)
check("it is no longer among the wallet's names", NAME not in mine())
refused("release a name nobody holds", "release", [NAME], "not registered")
register("register it again, to B, with nothing set", B, "")
check("nothing of the first registration came back", record()["npk"] is None and record()["texts"] == {})
change("release it again", "release", [NAME], lambda: record() is None)
change("withdraw A's published name", "set_primary", [A, ""], lambda: call("reverse", A) is None)

print("ALL PASS" if failures == 0 else "%d FAILED" % failures)
sys.exit(1 if failures else 0)
