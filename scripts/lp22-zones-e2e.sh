#!/bin/bash
# LP-0022 zones: two LEZ zones in one Names wallet, each with its own sequencer, over REAL
# Logos Core IPC (logosctl 0.3.2 and the names module from its lgx), the real names CLI and
# REAL standalone LEZ sequencers (v0.4.0-rc3, `--features standalone`). Nothing here is on a
# public chain: both chains are made by this script and thrown away with its work dir.
#
# Zone A is LEZ's networks/local.json as it is. Zone B is the same file with its own
# `bedrock_config.channel_id`: local.json and testnet.json name the same channel, and a
# zone is known by its channel.
#
# What it proves, in order:
#   1. home: the wallet is made and funded on A, a public send lands, home's channel is learnt
#   2. A goes away (a network drop); B is added as a second zone with its own sequencer,
#      gets its own wallet folder and accounts, is funded and sends
#   3. the module is restarted: both zones, the one in use and its balance are as before
#   4. B's sequencer is changed for another address of the same chain: nothing is lost, and
#      the next send goes through it
#   5. a sequencer of another chain is refused for B; back on home the balance is home's own
#      and home's account page lists nothing of B's
#   6. B is removed (its wallet folder stays) and added again: the wallet is found
#   7. the names CLI on its own (`names zone`): home is given A's sequencer, B is added and
#      found with the wallet the app left there, a send lands on B, B's sequencer is changed,
#      a command about a name is refused away from home, B is removed; the app's own
#      settings are untouched throughout
#
# One sequencer runs at a time, and every send is public (no proof is made). Runs on the
# Mac mini with bash, through the lock:
#   SEQ_BIN=<sequencer_service> SEQ_CONFIG=<networks/local.json> NAMES_BIN=<names CLI> \
#   NAMES_LGX=<names lgx dir> ZONES_PORT_BASE=3930 \
#     /Volumes/exos162/logos/dev/fix-names/mini-lock.sh /bin/bash scripts/lp22-zones-e2e.sh
#   ZONES_DIR   work dir, wiped and recreated (default /Volumes/exos162/logos/dev/lp22/zones/e2e-<base>)
# Ports: base..base+2 (A, B, B's second address), base+5..base+7 (their metrics).
# Exits non-zero on any FAIL.
set -uo pipefail
T=/Volumes/exos162/logos/dev/chain-ipc
BASE=${ZONES_PORT_BASE:-3930}
W=${ZONES_DIR:-/Volumes/exos162/logos/dev/lp22/zones/e2e-$BASE}
CTL=$T/logosctl-0.3.2/bin/logosctl
say() { printf '%s\n' "$*"; }
die() { say "ABORT $*"; exit 2; }
for p in $(seq "$BASE" $((BASE + 7))); do case $p in 8080|8081|3000|3001|3401|11434) die "port $p reserved";; esac; done
[ $((BASE + 7)) -ge 3597 ] && [ "$BASE" -le 3860 ] && die "ports in logos-34's range"
for v in SEQ_BIN SEQ_CONFIG NAMES_BIN NAMES_LGX; do [ -n "${!v:-}" ] && [ -e "${!v}" ] || die "set $v"; done
if lsof -nP -i :"$BASE" -i :$((BASE + 1)) -i :$((BASE + 2)) -i :$((BASE + 5)) -i :$((BASE + 6)) -i :$((BASE + 7)) >/dev/null 2>&1; then die "ports $BASE-$((BASE + 7)) busy"; fi
case "$W" in /Volumes/exos162/logos/names-0.4*|/Volumes/exos162/logos/dev/chain-ipc*|*wallet-testnet*|*user-test*) die "not in $W";; esac
if [ -d "$W" ] && [ -n "$(ls -A "$W")" ] && [ ! -e "$W/.lp22-zones-e2e" ]; then die "$W is not empty and not made by this script"; fi
rm -rf "$W"; mkdir -p "$W/home" "$W/session/modules" && touch "$W/.lp22-zones-e2e" || die "mkdir"
# The wallets here are throwaway, but a new wallet's recovery phrase passes through the
# session all the same: nobody else reads this folder.
chmod 700 "$W"
cd "$W" || die cd
# The sequencers keep the real home (their tools are found from it); everything else gets
# the work dir's, so the module's config and wallets are this run's alone.
export REAL_HOME=$HOME
export PATH=$REAL_HOME/.cargo/bin:$REAL_HOME/.risc0/bin:/opt/homebrew/bin:$PATH
export HOME=$W/home LOGOSCTL_CONFIG_DIR=$W/session
unset LEE_WALLET_HOME_DIR LEE_WALLET_PASSWORD LOGOS_NAMES_RPC LOGOS_NAMES_PROGRAM
say "lp22-zones-e2e $(date -u +%Y-%m-%dT%H:%M:%SZ) host $(hostname -s) work dir $W"
say "  names lgx $(readlink "$NAMES_LGX" || echo "$NAMES_LGX") sha256 $(shasum -a 256 "$NAMES_LGX"/*.lgx | cut -c1-64)"
say "  names CLI $NAMES_BIN sha256 $(shasum -a 256 "$NAMES_BIN" | cut -c1-64)"
say "  sequencer $SEQ_BIN sha256 $(shasum -a 256 "$SEQ_BIN" | cut -c1-64)"
STARTED=0
cleanup() {
  local rc=$?
  trap - EXIT INT TERM
  say ""; say "== stopping everything"
  # Only what this run started: its sequencers carry the work dir in their arguments.
  pkill -f "sequencer_service start $W/" 2>/dev/null || true
  [ $STARTED = 1 ] && { "$CTL" daemon stop || true; }
  local left=""
  for _ in $(seq 20); do left=$(pgrep -fl "$W" || true); [ -z "$left" ] && break; sleep 1; done
  if [ -n "$left" ]; then say "LEFT RUNNING: $left"; rc=1; else say "all stopped: pgrep -fl $W prints nothing"; fi
  exit $rc
}
trap cleanup EXIT
trap 'exit 130' INT TERM
"$CTL" daemon start --detach || die "daemon start"
STARTED=1
export CTL W BASE SEQ_BIN SEQ_CONFIG NAMES_BIN NAMES_LGX
python3 -I -u - <<'PY'
import hashlib, json, os, re, stat, subprocess, sys, time, urllib.request

CTL, W, SEQ_BIN, SEQ_CONFIG, NAMES_BIN = (os.environ[k] for k in ("CTL", "W", "SEQ_BIN", "SEQ_CONFIG", "NAMES_BIN"))
BASE = int(os.environ["BASE"])
URL_A, URL_B, URL_B2 = (f"http://127.0.0.1:{BASE + n}" for n in (0, 1, 2))
# LEZ's published local-test key (PRIVATE_KEY_PUB_ACC_A in lez/testnet_initial_state): local chains only.
LOCAL_TEST_KEY = "10a26a9aec7d34b82364eeae45c5294dbb0a764b000b94eeb9b58511dc487c4d"
CHANNEL_B = "b2" * 32
CONFIG = os.path.join(W, "home", ".config", "logos-names", "config.json")
os.chdir(W)
results = []


def say(text=""):
    print(text, flush=True)


def loads(text):
    if isinstance(text, str):
        try:
            return json.loads(text)
        except ValueError:
            return text
    return text


def call(module, method, *args, timeout=240):
    argv = [CTL, "--json", "call", module, method, *[str(a) for a in args]]
    t0 = time.time()
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return {"ok": False, "value": None, "error": f"timeout {timeout}s", "s": timeout}
    raw = p.stdout.strip()
    dt = round(time.time() - t0, 2)
    try:
        out = json.loads(raw)
    except ValueError:
        return {"ok": False, "value": None, "error": f"not JSON: {raw[:300]} {p.stderr.strip()[:300]}", "s": dt}
    if out.get("status") != "ok":
        return {"ok": False, "value": None, "error": out.get("message") or raw[:300], "s": dt}
    r = loads(out.get("result"))
    if not isinstance(r, dict) or "success" not in r:
        return {"ok": True, "value": r, "error": None, "s": dt}
    return {"ok": bool(r["success"]), "value": loads(r.get("value")), "error": r.get("error"), "warning": r.get("warning"), "s": dt}


def names(method, *args, **kw):
    return call("names", method, *args, **kw)


def hidden(value):
    """`value` without a recovery phrase: only how many words it has."""
    if isinstance(value, dict):
        return {k: (f"<{len(str(v).split())} words, not shown>" if k == "phrase" and v else hidden(v)) for k, v in value.items()}
    if isinstance(value, list):
        return [hidden(v) for v in value]
    return value


def shown(r, n=900):
    s = json.dumps(hidden(r["value"]), sort_keys=True) if r["ok"] else f"ERR {r['error']}"
    return s[:n]


def show(label, r, n=900):
    say(f"    {label} ({r.get('s')}s): {shown(r, n)}")


def step(name, ok, detail=""):
    line = f"{'PASS' if ok else 'FAIL'} {name}" + (f" — {detail}" if detail else "")
    say(line)
    results.append(line)
    return ok


def ctl(*argv):
    p = subprocess.run([CTL, *argv], capture_output=True, text=True, timeout=120, stdin=subprocess.DEVNULL)
    return p.returncode, (p.stdout + p.stderr).strip()


def finish():
    say(f"RESULT {sum(1 for r in results if r.startswith('PASS'))} PASS, {sum(1 for r in results if r.startswith('FAIL'))} FAIL")
    sys.exit(1 if any(r.startswith("FAIL") for r in results) else 0)


def abort(why):
    say(f"ABORT {why}")
    results.append(f"FAIL aborted: {why}")
    finish()


# ── the sequencers: one at a time ──
running = {}


def rpc(url, method, params=None, wait=10):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": [] if params is None else params}).encode()
    request = urllib.request.Request(url, body, {"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=wait) as reply:
        return json.load(reply).get("result")


def answers(url):
    try:
        return rpc(url, "getLastBlockId", wait=3) is not None
    except Exception:
        return False


def seq_setup(home, config):
    p = subprocess.run([SEQ_BIN, "setup", home, "--config", config, "--funding-public-key", "01" * 31 + "00", "--create-channel"],
                       capture_output=True, text=True, timeout=300, stdin=subprocess.DEVNULL, env=dict(os.environ, HOME=os.environ["REAL_HOME"]))
    open(home + "-setup.log", "w").write(p.stdout + p.stderr)
    if p.returncode != 0:
        abort(f"sequencer setup {home}: {(p.stdout + p.stderr)[-400:]}")


def seq_up(home, url, metrics):
    if running:
        abort(f"a sequencer is already up ({list(running)}): one at a time")
    port = url.rsplit(":", 1)[1]
    log = open(home + ".log", "a")
    p = subprocess.Popen([SEQ_BIN, "start", home, "--rpc-listen-address", f"127.0.0.1:{port}", "--metrics-listen-address", f"127.0.0.1:{metrics}"],
                         stdout=log, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL, env=dict(os.environ, HOME=os.environ["REAL_HOME"], RUST_LOG="warn"))
    running[url] = p
    for _ in range(90):
        if answers(url):
            say(f"    sequencer {os.path.basename(home)} up on {url} (pid {p.pid}), block {rpc(url, 'getLastBlockId')}, channel {rpc(url, 'getChannelId', {})}")
            return
        if p.poll() is not None:
            break
        time.sleep(2)
    abort(f"the sequencer in {home} did not answer on {url}: {open(home + '.log', errors='replace').read()[-600:]}")


def seq_down(url):
    p = running.pop(url)
    p.terminate()
    try:
        p.wait(40)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait(10)
    for _ in range(20):
        if not answers(url):
            break
        time.sleep(1)
    say(f"    sequencer on {url} stopped")


# ── the names CLI, used directly: the funded genesis account and what the chain says ──
def cli(wallet, url, *args, timeout=240):
    env = dict(os.environ, LEE_WALLET_HOME_DIR=wallet, LOGOS_NAMES_RPC=url, LEE_WALLET_PASSWORD="")
    p = subprocess.run([NAMES_BIN, *args], capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL, env=env)
    return p.returncode, p.stdout, p.stderr


def funder(wallet, url):
    """A wallet that holds the chain's funded local-test account; returns the account."""
    os.makedirs(wallet, mode=0o700)
    json.dump({"gas_limit": 2000000, "fee_headroom_blocks": 5, "seq_block_poll_max_amount": 100, "seq_poll_max_retries": 5,
               "seq_poll_timeout": "12s", "seq_tx_poll_max_blocks": 5, "sequencer": {"sequencer_addr": url + "/"}},
              open(os.path.join(wallet, "wallet_config.json"), "w"))
    rc, out, err = cli(wallet, url, "wallet", "check-health")
    if rc != 0:
        abort(f"wallet check-health in {wallet}: {err[-300:]}")
    rc, out, err = cli(wallet, url, "wallet", "account", "import", "public", "--private-key", LOCAL_TEST_KEY)
    found = re.search(r"Public/([1-9A-HJ-NP-Za-km-z]{32,44})", out)
    if not found:
        abort(f"import of the local-test key: {err[-300:]}")
    return found.group(1)


def on_chain(wallet, url, account):
    """A public account's native balance, as the chain has it."""
    rc, out, err = cli(wallet, url, "wallet", "account", "get", "--account-id", f"Public/{account}")
    found = re.search(r"^Balance (\d+)", out, re.M)
    return int(found.group(1)) if found else None


def public_key(wallet, url, account):
    rc, out, err = cli(wallet, url, "wallet", "account", "get", "--account-id", f"Public/{account}", "--keys")
    found = re.search(r"^pk (\S+)", out, re.M)
    return found.group(1) if found else None


def fund(from_wallet, url, source, to_wallet, to, amount):
    """Pays a new account of another wallet: it has never been used, so the send carries its public key."""
    pk = public_key(to_wallet, url, to)
    if not pk:
        abort(f"no public key for Public/{to}")
    rc, out, err = cli(from_wallet, url, "wallet", "auth-transfer", "send", "--from", f"Public/{source}", "--to-pk", pk, "--amount", str(amount))
    lines = [line for line in out.splitlines() if line.startswith("Transaction ")]
    say(f"    funded Public/{to} with {amount}: exit {rc} {' / '.join(lines)[:200]} {err.strip()[-200:] if rc else ''}")
    return rc == 0


# ── the module ──
def public_of(r):
    """The default public account's balance from a `cli_balance` reply."""
    try:
        return int(r["value"]["public"]["balance"])
    except (KeyError, TypeError, ValueError):
        return None


def balance(tries=8, want=None):
    """`cli_balance`, read again while the sequencer is still catching the wallet up."""
    r = names("cli_balance")
    for _ in range(tries):
        if r["ok"] and public_of(r) is not None and (want is None or public_of(r) == want):
            break
        time.sleep(3)
        r = names("cli_balance")
    return r


def job_end(started, seconds=240):
    if not started["ok"] or not isinstance(started["value"], dict) or not started["value"].get("id"):
        return started
    end = time.time() + seconds
    r = names("job", started["value"]["id"])
    while r["ok"] and r["value"].get("state") == "running" and time.time() < end:
        time.sleep(2)
        r = names("job", started["value"]["id"])
    return r


def zone_view():
    r = names("zone_list")
    return r["value"] if r["ok"] and isinstance(r["value"], dict) else {"zones": [], "current": None}


def zone_line(view):
    return "; ".join(f"{z['name']} [{z['id'][:8]}{'…' if len(z['id']) > 8 else ''}] {z['sequencer']}{' IN USE' if z['current'] else ''}{'' if z['has_wallet'] else ' (no wallet)'}"
                     for z in view.get("zones", []))


def saved():
    try:
        return json.load(open(CONFIG))
    except (OSError, ValueError):
        return {}


def storage_hash(folder):
    try:
        return hashlib.sha256(open(os.path.join(folder, "storage.json"), "rb").read()).hexdigest()[:16]
    except OSError:
        return None


say(f"== session {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} ports {BASE}-{BASE + 7}")
config_b = os.path.join(W, "zone-b.json")
template = json.load(open(SEQ_CONFIG))
CHANNEL_A = template["bedrock_config"]["channel_id"]
template["bedrock_config"]["channel_id"] = CHANNEL_B
json.dump(template, open(config_b, "w"), indent=1)
SEQ_A, SEQ_B = os.path.join(W, "seq-a"), os.path.join(W, "seq-b")
seq_setup(SEQ_A, SEQ_CONFIG)
seq_setup(SEQ_B, config_b)
say(f"    two chains made: A channel {CHANNEL_A[:8]}… (local.json as it is), B channel {CHANNEL_B[:8]}… (its own)")
rc, out = ctl("install", "--dir", os.environ["NAMES_LGX"], "-y")
say(f"    install names: exit {rc} {out[-200:] if rc else ''}")
rc, out = ctl("module", "load", "names")
say(f"    module load names: {out}")
if rc != 0:
    abort("module load")

# ── 1. home ──
say("\n== 1. HOME: zone A, the wallet made and funded there")
seq_up(SEQ_A, URL_A, BASE + 5)
step("the names CLI is taken", names("set_cli", NAMES_BIN)["value"] is True)
r = names("set_sequencer", URL_A)
step("home reads from sequencer A", r["value"] is True, shown(r))
r = names("cli_status")
show("cli_status", r)
HOME_WALLET = r["value"].get("wallet_dir") if r["ok"] else None
step("home has no wallet yet, and would keep it in this run's own folder", r["ok"] and r["value"]["wallet"] is False and str(HOME_WALLET).startswith(W), str(HOME_WALLET))
made_home = names("cli_create_wallet", timeout=300)
show("cli_create_wallet", made_home)
P_HOME = made_home["value"].get("public") if made_home["ok"] else None
step("home's wallet is made", made_home["ok"] and bool(P_HOME) and bool(made_home["value"].get("phrase")), f"Public/{P_HOME}")
if not P_HOME:
    abort("no home wallet")
FUND_A = os.path.join(W, "funder-a")
F = funder(FUND_A, URL_A)
say(f"    the funded local-test account: Public/{F}, holds {on_chain(FUND_A, URL_A, F)} on A")
step("home's public account is funded on A", fund(FUND_A, URL_A, F, HOME_WALLET, P_HOME, 5_000_000_000))
r = balance(want=5_000_000_000)
show("cli_balance", r, 400)
step("the module reads home's balance from A", public_of(r) == 5_000_000_000, str(public_of(r)))
f_before = on_chain(FUND_A, URL_A, F)
sent = names("cli_pay_account", f"Public/{F}", "1,000", "false")
show("cli_pay_account F 1,000", sent)
ended = job_end(sent)
show("job", ended, 500)
H1 = public_of(balance())
f_after = on_chain(FUND_A, URL_A, F)
step("a public send on home lands on A", ended["ok"] and ended["value"].get("state") == "done" and f_before is not None and f_after == f_before + 1000,
     f"F {f_before} -> {f_after}; home holds {H1}")
view = zone_view()
say(f"    zones: {zone_line(view)}")
step("one zone, home, and its channel is learnt from its sequencer",
     len(view["zones"]) == 1 and view["zones"][0]["home"] and view["zones"][0]["channel"] == CHANNEL_A and view["current"] == "home", str(view["zones"][0].get("channel")) if view["zones"] else "")

# ── 2. a second zone ──
say("\n== 2. A SECOND ZONE: A is gone (a network drop), B is added with its own sequencer")
seq_down(URL_A)
r = names("cli_balance")
show("cli_balance with home's sequencer down", r, 300)
view = zone_view()
step("home's sequencer down: the zones still answer, and nothing is invented for the balance",
     len(view["zones"]) == 1 and view["zones"][0]["channel"] == CHANNEL_A and (not r["ok"] or r.get("warning") or public_of(r) in (None, H1)), r["error"] or str(r.get("warning")))
seq_up(SEQ_B, URL_B, BASE + 6)
r = names("zone_add", "Second", URL_B)
show("zone_add Second", r)
ZB = r["value"].get("added") if r["ok"] else None
view = r["value"] if r["ok"] else {"zones": []}
step("B is added as a zone, known by its own channel, home still in use",
     ZB == CHANNEL_B and len(view["zones"]) == 2 and view["current"] == "home" and view["zones"][1]["sequencer"] == URL_B and view["zones"][1]["has_wallet"] is False, zone_line(view))
if ZB != CHANNEL_B:
    abort("zone B not added")
r = names("zone_add", "Again", URL_B)
step("the same chain can't be added twice", not r["ok"] and "already set up" in str(r["error"]), str(r["error"]))
r = names("zone_sequencer", "home", URL_B, "false")
step("home can't take the sequencer of another zone here", not r["ok"] and len(zone_view()["zones"]) == 2 and saved().get("sequencer") == URL_A, str(r["error"]))
r = names("zone_use", ZB)
show("zone_use Second", r, 300)
config = names("get_config")
step("B is the zone in use, and get_config says so", r["ok"] and r["value"]["current"] == ZB and config["ok"] and config["value"]["zone"]["name"] == "Second"
     and config["value"]["sequencer"] == URL_B and config["value"]["names_sequencer"] == URL_A, json.dumps(config["value"].get("zone")) if config["ok"] else str(config["error"]))
r = names("cli_status")
B_WALLET = r["value"].get("wallet_dir") if r["ok"] else None
step("B has its own wallet folder, with no wallet in it yet", r["ok"] and r["value"]["wallet"] is False and B_WALLET == os.path.join(os.path.dirname(CONFIG), "zones", ZB, "wallet"), str(B_WALLET))
r = names("cli_register", "zonetest", "false")
step("a name can't be registered off home: names are kept there", not r["ok"] and "names are kept on" in str(r["error"]), str(r["error"]))
home_before = storage_hash(HOME_WALLET)
made_b = names("cli_create_wallet", timeout=300)
show("cli_create_wallet on Second", made_b)
P_B = made_b["value"].get("public") if made_b["ok"] else None
step("B's wallet is its own: other accounts, another recovery phrase, home's wallet file untouched",
     bool(P_B) and P_B != P_HOME and made_b["value"].get("private") != made_home["value"].get("private") and bool(made_b["value"].get("phrase"))
     and made_b["value"].get("phrase") != made_home["value"].get("phrase") and storage_hash(HOME_WALLET) == home_before and storage_hash(B_WALLET) is not None,
     f"Public/{P_B} in {B_WALLET}")
if not P_B:
    abort("no wallet on B")
mode = stat.S_IMODE(os.stat(os.path.dirname(B_WALLET)).st_mode)
step("B's wallet folder is the owner's alone", mode & 0o077 == 0 and stat.S_IMODE(os.stat(B_WALLET).st_mode) & 0o077 == 0, oct(mode))
FUND_B = os.path.join(W, "funder-b")
F_B = funder(FUND_B, URL_B)
step("B's public account is funded on B", F_B == F and fund(FUND_B, URL_B, F, B_WALLET, P_B, 3_000_000_000))
r = balance(want=3_000_000_000)
show("cli_balance on Second", r, 400)
step("the module reads B's balance from B's sequencer", public_of(r) == 3_000_000_000, str(public_of(r)))
f_before = on_chain(FUND_B, URL_B, F)
sent = names("cli_pay_account", f"Public/{F}", "2,500", "false")
show("cli_pay_account F 2,500 on Second", sent)
ended = job_end(sent)
show("job", ended, 500)
B1 = public_of(balance())
f_after = on_chain(FUND_B, URL_B, F)
step("a public send on B lands on B", ended["ok"] and ended["value"].get("state") == "done" and f_before is not None and f_after == f_before + 2500
     and B1 is not None and B1 <= 3_000_000_000 - 2500, f"F {f_before} -> {f_after}; B holds {B1}")
r = names("activity", "")
rows = r["value"] if r["ok"] and isinstance(r["value"], list) else []
b_sends = [e for e in rows if e.get("kind") == "pay_account" and e.get("zone") == ZB]
home_sends = [e for e in rows if e.get("kind") == "pay_account" and not e.get("zone")]
say(f"    history: {[(e.get('kind'), e.get('amount'), e.get('zone', 'home')[:8], e.get('zone_name')) for e in rows]}")
step("the history marks B's send with B's zone and home's with none",
     len(b_sends) == 1 and b_sends[0].get("zone_name") == "Second" and str(b_sends[0].get("amount")).replace(",", "") == "2500"
     and len(home_sends) == 1 and home_sends[0].get("zone_name") == "Home", f"{len(b_sends)} on B, {len(home_sends)} on home")

# ── 3. restart ──
say("\n== 3. RESTART: the daemon and the module are stopped and started again")
before = (saved().get("zone"), [z.get("channel") for z in saved().get("zones", [])], storage_hash(B_WALLET), storage_hash(HOME_WALLET))
say("    daemon stop: " + ctl("daemon", "stop")[1][-120:])
time.sleep(2)
say("    daemon start: " + ctl("daemon", "start", "--detach")[1][-120:])
rc, out = ctl("module", "load", "names")
say(f"    module load names: {out}")
view = zone_view()
say(f"    zones: {zone_line(view)}")
r = balance()
step("after a restart: both zones, B in use with its sequencer, wallet and balance as before",
     rc == 0 and len(view["zones"]) == 2 and view["current"] == ZB and view["zones"][1]["sequencer"] == URL_B and view["zones"][1]["has_wallet"] is True
     and view["zones"][0]["channel"] == CHANNEL_A and public_of(r) == B1
     and (saved().get("zone"), [z.get("channel") for z in saved().get("zones", [])], storage_hash(HOME_WALLET)) == (before[0], before[1], before[3]),
     f"B holds {public_of(r)}")
r = names("cli_status")
step("the wallet on B is still B's", r["ok"] and r["value"]["public"] == P_B and r["value"]["wallet_dir"] == B_WALLET, str(r["value"].get("public")) if r["ok"] else str(r["error"]))

# ── 4. another sequencer for the same zone ──
say("\n== 4. SEQUENCER SWITCH: B's chain is served from another address")
seq_down(URL_B)
r = names("cli_balance")
show("cli_balance with B's sequencer down", r, 300)
step("B's sequencer down: no balance is invented", not r["ok"] or r.get("warning") or public_of(r) in (None, B1), r["error"] or str(r.get("warning")))
# The amount has a comma so that logosctl passes it as text: a bare 700 goes as a number, and
# the call is then refused for its argument, not for the sequencer.
sent = names("cli_pay_account", f"Public/{F}", "1,700", "false")
step("B's sequencer down: a send is refused before anything starts",
     not sent["ok"] and "Nothing was started" in str(sent["error"]) and names("running_jobs")["value"] in ([], None), str(sent["error"]))
r = names("set_sequencer", URL_B2)
step("an address that doesn't answer isn't taken for B", r["value"] is False and zone_view()["zones"][1]["sequencer"] == URL_B)
seq_up(SEQ_B, URL_B2, BASE + 7)
r = names("zone_sequencer", ZB, URL_B2, "false")
show("zone_sequencer Second -> its second address", r, 300)
view = zone_view()
step("B takes the other sequencer of its own chain", r["ok"] and r["value"].get("changed") is True and r["value"].get("channel") == CHANNEL_B
     and view["zones"][1]["sequencer"] == URL_B2 and view["current"] == ZB and saved().get("sequencer") == URL_A, zone_line(view))
r = balance()
status = names("cli_status")
step("after the switch: the same wallet, accounts and balance", public_of(r) == B1 and status["ok"] and status["value"]["public"] == P_B and status["value"]["wallet_dir"] == B_WALLET, f"B holds {public_of(r)}")
rows = names("activity", f"Public/{P_B}")
rows = rows["value"] if rows["ok"] and isinstance(rows["value"], list) else []
step("after the switch: B's history is still there", any(e.get("kind") == "pay_account" and e.get("zone") == ZB for e in rows), f"{len(rows)} entries on B's public account")
f_before = on_chain(FUND_B, URL_B2, F)
sent = names("cli_pay_account", f"Public/{F}", "1,500", "false")
ended = job_end(sent)
show("job", ended, 500)
B2 = public_of(balance())
f_after = on_chain(FUND_B, URL_B2, F)
step("a send through the new sequencer lands", ended["ok"] and ended["value"].get("state") == "done" and f_before is not None and f_after == f_before + 1500 and B2 is not None and B2 <= B1 - 1500,
     f"F {f_before} -> {f_after}; B holds {B2}")

# ── 5. another chain is another zone ──
say("\n== 5. ANOTHER CHAIN IS REFUSED, and home is still home")
seq_down(URL_B2)
seq_up(SEQ_A, URL_A, BASE + 5)
r = names("zone_sequencer", ZB, URL_A, "false")
view = zone_view()
step("B can't take a sequencer of another chain", not r["ok"] and view["zones"][1]["sequencer"] == URL_B2 and view["zones"][1]["channel"] == CHANNEL_B, str(r["error"]))
r = names("zone_use", "home")
step("back on home", r["ok"] and r["value"]["current"] == "home", zone_line(r["value"]) if r["ok"] else str(r["error"]))
r = balance()
status = names("cli_status")
step("home's wallet and balance are home's own, untouched by B", public_of(r) == H1 and status["ok"] and status["value"]["public"] == P_HOME and status["value"]["wallet_dir"] == HOME_WALLET
     and on_chain(FUND_A, URL_A, P_B) in (None, 0), f"home holds {public_of(r)}; B's account on A holds {on_chain(FUND_A, URL_A, P_B)}")
rows = names("activity", f"Public/{F}")
rows = rows["value"] if rows["ok"] and isinstance(rows["value"], list) else []
step("an account's page on home lists home's sends only (F was paid on both zones)",
     [str(e.get("amount")).replace(",", "") for e in rows if e.get("kind") == "pay_account"] == ["1000"] and not any(e.get("zone") for e in rows),
     str([(e.get("kind"), e.get("amount"), e.get("zone", "home")[:8]) for e in rows]))
r = names("zone_use", ZB)
rows = names("activity", f"Public/{F}")
rows = rows["value"] if rows["ok"] and isinstance(rows["value"], list) else []
step("the same account's page on B lists B's sends only (B's sequencer is down: the page is still read)",
     sorted(str(e.get("amount")).replace(",", "") for e in rows if e.get("kind") == "pay_account") == ["1500", "2500"] and all(e.get("zone") == ZB for e in rows if e.get("kind") == "pay_account"),
     str([(e.get("kind"), e.get("amount"), e.get("zone", "home")[:8]) for e in rows]))
names("zone_use", "home")

# ── 6. remove, and add again ──
say("\n== 6. REMOVE AND ADD AGAIN")
r = names("zone_remove", "home")
step("home can't be removed", not r["ok"] and len(zone_view()["zones"]) == 2, str(r["error"]))
names("zone_use", ZB)
r = names("zone_remove", ZB)
step("the zone in use can't be removed", not r["ok"] and len(zone_view()["zones"]) == 2, str(r["error"]))
names("zone_use", "home")
kept = storage_hash(B_WALLET)
r = names("zone_remove", ZB)
show("zone_remove Second", r, 400)
step("B is removed, and its wallet folder stays where it is", r["ok"] and len(r["value"]["zones"]) == 1 and r["value"]["removed"]["wallet_dir"] == B_WALLET
     and storage_hash(B_WALLET) == kept and kept is not None and "zones" in saved() and len(saved()["zones"]) == 1, str(r["value"].get("removed")) if r["ok"] else str(r["error"]))
rows = names("activity", "")
rows = rows["value"] if rows["ok"] and isinstance(rows["value"], list) else []
gone = [e for e in rows if e.get("zone") == ZB]
step("the removed zone's history is kept, marked as a zone that is no longer here", len(gone) >= 2 and all("zone_name" in e and e["zone_name"] is None for e in gone), f"{len(gone)} entries")
r = balance()
step("home works as before with one zone", public_of(r) == H1, f"home holds {public_of(r)}")
seq_down(URL_A)
seq_up(SEQ_B, URL_B2, BASE + 7)
r = names("zone_add", "Second again", URL_B2)
view = r["value"] if r["ok"] else {"zones": []}
step("added again (home's sequencer is down; its channel is known): the wallet left behind is found",
     r["ok"] and r["value"]["added"] == ZB and len(view["zones"]) == 2 and view["zones"][1]["has_wallet"] is True and view["zones"][1]["wallet_dir"] == B_WALLET, zone_line(view) if r["ok"] else str(r["error"]))
names("zone_use", ZB)
r = balance()
status = names("cli_status")
step("and it holds what it held", public_of(r) == B2 and status["ok"] and status["value"]["public"] == P_B, f"B holds {public_of(r)}")
names("zone_use", "home")
seq_down(URL_B2)

# ── 7. the CLI's own zones ──
say("\n== 7. THE CLI: names zone, with no --rpc and no wallet folder given")
CLI_ZONES = os.path.join(W, "home", ".config", "logos-names", "cli-zones.json")
app_before = saved()


def zcli(*args, timeout=240):
    """`names` as a person types it: the zone decides the sequencer and the wallet folder."""
    env = {k: v for k, v in os.environ.items() if k not in ("LOGOS_NAMES_RPC", "LEE_WALLET_HOME_DIR", "LOGOS_NAMES_ZONE")}
    env["LEE_WALLET_PASSWORD"] = ""
    p = subprocess.run([NAMES_BIN, *args], capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL, env=env)
    return p.returncode, p.stdout.strip(), p.stderr.strip()


def zlist():
    rc, out, err = zcli("zone", "list", "--json")
    try:
        return json.loads(out)
    except ValueError:
        return {"zones": [], "current": None, "error": err[-200:]}


def held(out, account):
    found = re.search(rf"Public/{account}\s+([\d,]+)", out)
    return int(found.group(1).replace(",", "")) if found else None


seq_up(SEQ_A, URL_A, BASE + 5)
# Home is the public testnet until told otherwise; this run stays on its own chains.
rc, out, err = zcli("zone", "sequencer", "home", URL_A, "--other-chain")
view = zlist()
step("CLI: home takes A's sequencer, and its channel is learnt", rc == 0 and len(view["zones"]) == 1 and view["zones"][0]["sequencer"] == URL_A
     and view["zones"][0]["channel"] == CHANNEL_A and view["current"] == "home", (out or err)[-200:])
rc, out, err = zcli("balance")
step("CLI: on home, balance is home's wallet on A", rc == 0 and held(out, P_HOME) == H1 and "zone:" not in err, f"home holds {held(out, P_HOME)}; exit {rc} {err[-200:] if rc else ''}")
seq_down(URL_A)
seq_up(SEQ_B, URL_B2, BASE + 7)
rc, out, err = zcli("zone", "add", URL_A, "--name", "Nowhere")
step("CLI: a sequencer that doesn't answer isn't added", rc != 0 and len(zlist()["zones"]) == 1, err[-200:])
rc, out, err = zcli("zone", "add", URL_B2, "--name", "Second CLI")
view = zlist()
step("CLI: B is added with its channel, and the wallet the app left there is found (home's sequencer is down)",
     rc == 0 and len(view["zones"]) == 2 and view["zones"][1]["id"] == ZB and view["zones"][1]["wallet_dir"] == B_WALLET and view["zones"][1]["has_wallet"] is True
     and view["current"] == "home" and "already in" in out, out[-300:] if rc == 0 else err[-300:])
rc, out, err = zcli("--zone", "b2b2", "balance")
step("CLI: --zone runs one command on B: B's wallet, B's balance, and it says which zone", rc == 0 and held(out, P_B) == B2 and f"zone: Second CLI ({URL_B2})" in err,
     f"B holds {held(out, P_B)}; {err[-120:]}")
rc, out, err = zcli("zone", "use", "second cli")
rc2, out2, err2 = zcli("balance")
step("CLI: B in use: every command runs there", rc == 0 and zlist()["current"] == ZB and rc2 == 0 and held(out2, P_B) == B2, out[-200:] if rc == 0 else err[-200:])
rc, out, err = zcli("resolve", "jef")
step("CLI: a command about a name is refused away from home", rc != 0 and "names are kept on Home" in err and "Nothing was started" in err, err[-200:])
rc, out, err = zcli("--zone", "home", "--rpc", URL_B2, "balance")
step("CLI: --zone and --rpc together are refused", rc != 0 and "not both" in err, err[-200:])
f_before = on_chain(FUND_B, URL_B2, F)
rc, out, err = zcli("wallet", "auth-transfer", "send", "--from", f"Public/{P_B}", "--to", f"Public/{F}", "--amount", "300")
f_after = on_chain(FUND_B, URL_B2, F)
rc2, out2, err2 = zcli("balance")
B3 = held(out2, P_B)
step("CLI: a public send on B lands on B", rc == 0 and f_before is not None and f_after == f_before + 300 and B3 is not None and B3 <= B2 - 300,
     f"F {f_before} -> {f_after}; B holds {B3}; {err[-200:] if rc else ''}")
seq_down(URL_B2)
seq_up(SEQ_B, URL_B, BASE + 6)
rc, out, err = zcli("zone", "sequencer", "second cli", URL_A)
step("CLI: B doesn't take an address that doesn't answer", rc != 0 and zlist()["zones"][1]["sequencer"] == URL_B2, err[-200:])
kept = storage_hash(B_WALLET)
rc, out, err = zcli("zone", "sequencer", "second cli", URL_B)
view = zlist()
rc2, out2, err2 = zcli("balance")
step("CLI: B takes another sequencer of its own chain: the same wallet and balance", rc == 0 and "the same chain" in out and view["zones"][1]["sequencer"] == URL_B
     and view["zones"][1]["channel"] == CHANNEL_B and rc2 == 0 and held(out2, P_B) == B3, out[-200:] if rc == 0 else err[-200:])
rc, out, err = zcli("zone", "remove", "second cli")
step("CLI: the zone in use can't be removed", rc != 0 and len(zlist()["zones"]) == 2, err[-200:])
zcli("zone", "use", "home")
kept = storage_hash(B_WALLET)
rc, out, err = zcli("zone", "remove", "second cli")
view = zlist()
step("CLI: B is removed, and its wallet folder stays", rc == 0 and len(view["zones"]) == 1 and view["current"] == "home" and B_WALLET in out and storage_hash(B_WALLET) == kept and kept is not None,
     out[-200:] if rc == 0 else err[-200:])
step("CLI: its list is its own file, the owner's alone, and the app's settings are as they were",
     saved() == app_before and stat.S_IMODE(os.stat(CLI_ZONES).st_mode) & 0o077 == 0, f"{oct(stat.S_IMODE(os.stat(CLI_ZONES).st_mode))}; app zones {[z.get('name') for z in saved().get('zones', [])]}")
seq_down(URL_B)

final = saved()
say(f"\n    config.json: sequencer {final.get('sequencer')}, zone {final.get('zone')}, zones {[(z.get('name'), str(z.get('channel'))[:8], z.get('sequencer')) for z in final.get('zones', [])]}")
step("the saved configuration is the owner's alone", stat.S_IMODE(os.stat(CONFIG).st_mode) & 0o077 == 0, oct(stat.S_IMODE(os.stat(CONFIG).st_mode)))
finish()
PY
