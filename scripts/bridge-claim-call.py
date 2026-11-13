#!/usr/bin/env python3
"""Builds the `propose_private_call` JSON of an RFP-022 private attested claim (a test stand-in
for the bridge mini-app), from a demo run's frozen material. Prints nothing secret: the call
file holds the claim secret k inside its message, so it is written mode 0600 and only its size
and digest are printed.

    bridge-claim-call.py <run dir> <leaf index> <secret env name> <claimer V (base58)> <out.json>

e.g. on the Mac mini, for the v2 demo's unclaimed leaf 1 (V's 100 dUSDC, K_V100B):
    bridge-claim-call.py /Volumes/exos162/logos/rfp-022/run/sepolia2 1 K_V100B \
        2if1EJVcZwFT3oXxF4uewB2mk2XDXpmCWwTtBwUQ6HUC /tmp/x/claim-leaf1.json

The encoding follows rfp-022 lez-bridge (bridge/src/lib.rs `Message::Instruction(ClaimAttested)`,
cli/src/main.rs `ClaimPlan::mentions`); the wallet's own simulation is what proves it right.
"""
import hashlib, json, os, struct, sys

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

def b58decode(text):
    n = 0
    for c in text:
        n = n * 58 + B58.index(c)
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big") if n else b""
    pad = len(text) - len(text.lstrip("1"))
    out = b"\0" * pad + raw
    assert len(out) == 32, "an account id is 32 bytes"
    return out

def b58encode(raw):
    n = int.from_bytes(raw, "big")
    out = ""
    while n:
        n, r = divmod(n, 58)
        out = B58[r] + out
    return "1" * (len(raw) - len(raw.lstrip(b"\0"))) + out

def sha(*parts):
    return hashlib.sha256(b"".join(parts)).digest()

def hx(text):
    text = text.strip()
    text = text[2:] if text.startswith("0x") else text
    return bytes.fromhex(text)

def word32(text):
    t = text.strip()
    t = t[2:] if t.startswith("0x") else t
    return bytes.fromhex(t.rjust(64, "0"))

def u256(v):
    return v.to_bytes(32, "big")

def env_file(path):
    out = {}
    for line in open(path):
        line = line.strip()
        if "=" in line and not line.startswith("#"):
            k, v = line.split("=", 1)
            out[k.strip()] = v.strip()
    return out

# borsh
def u8(v): return struct.pack("<B", v)
def u32(v): return struct.pack("<I", v)
def u64(v): return struct.pack("<Q", v)
def u128(v): return v.to_bytes(16, "little")
def vec_bytes(b): return u32(len(b)) + b
def vec_vec(items): return u32(len(items)) + b"".join(vec_bytes(i) for i in items)
def vec_fixed(items): return u32(len(items)) + b"".join(items)
def some(b): return u8(1) + b

DEPOSIT_LEAF_TAG = hx("9e8d098bfe4f2807258ba6b200534afcacae50702d4735cb40a3f31e9bea950c")
CONFIG_SEED = b"/RFP022/bridge/config/v1/000000/"
NULLIFIER_SEED_DOMAIN = b"/RFP022/bridge/claim-nullifier/v1"
ANCHOR_SEED_DOMAIN = b"/RFP021/attest/anchor/v1"
DEPOSIT_ROOT_SLOT = 83

def main():
    run, index, secret_name, claimer, out = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5]
    ids = env_file(os.path.join(run, "ids.env"))
    com = env_file(os.path.join(run, "commitments.env"))
    chk = env_file(os.path.join(run, "attested", "checkpoint.env"))
    k = hx(env_file(os.path.join(run, "secrets.env"))[secret_name])
    assert len(k) == 32
    V = b58decode(claimer)
    token = hx(com["USDC"]); cfg = hx(com["CFG_ID"]); vault = hx(com["VAULT"])
    denom, denom_index = 100_000_000, 0
    c = sha(b"RFP022/dep", k, V, token, u256(denom), cfg)
    leaves = [hx(l) for l in open(os.path.join(run, "attested", "leaves.txt")) if l.strip()]
    leaf = sha(DEPOSIT_LEAF_TAG, cfg, token, u256(denom), c)
    assert leaves[index] == leaf, "the secret and claimer don't give this leaf"
    zeros = [b"\0" * 32]
    for _ in range(32):
        zeros.append(sha(zeros[-1], zeros[-1]))
    layer, idx, path = list(leaves), index, []
    for h in range(32):
        sib = idx ^ 1
        path.append(layer[sib] if sib < len(layer) else zeros[h])
        nxt = []
        for i in range(0, max(len(layer), 1), 2):
            left = layer[i] if i < len(layer) else zeros[h]
            right = layer[i + 1] if i + 1 < len(layer) else zeros[h]
            nxt.append(sha(left, right))
        layer, idx = nxt, idx // 2
    node = leaf
    for h, s in enumerate(path):
        node = sha(s, node) if (index >> h) & 1 else sha(node, s)
    root = node
    nf = sha(b"RFP022/nfc", k)
    opening = sha(b"RFP022/alias", k)
    instance = hx(chk["INSTANCE_A"]); anchor = hx(chk["ANCHOR_A"])
    AP = b58decode(ids["AP"]); BP = ids["BP"]; TP = ids["TP"]
    proof = json.load(open(os.path.join(run, "attested", "slot-A", "eth_getProof.json")))
    pf = proof.get("result", proof)
    assert hx(pf["address"]) == vault
    sp = pf["storageProof"][0]
    header = hx(open(os.path.join(run, "attested", "slot-A", "header.rlp.hex")).read())
    storage_proof = (
        u64(int(pf["nonce"], 16)) + word32(pf["balance"]) + word32(pf["storageHash"]) + word32(pf["codeHash"])
        + vec_vec([hx(n) for n in pf["accountProof"]])
        + word32(sp["value"]) + vec_vec([hx(n) for n in sp["proof"]])
    )
    message = (
        u8(0)  # Message::Instruction
        + u8(5)  # Instruction::ClaimAttested
        + token + u32(denom_index) + u128(denom) + root + u64(index) + vec_fixed(path) + k + cfg
        + AP + instance + u64(DEPOSIT_ROOT_SLOT)  # AttestSource
        + vault + u64(11155111)
        + u8(0) + anchor + some(vec_bytes(header))  # AnchorWitness { BlockHash, Some(header) }
        + storage_proof
        + some(opening)
    )
    anchor_seed = sha(ANCHOR_SEED_DOMAIN, instance, bytes([1]), anchor)
    call = {
        "programs": [BP, TP, ids["AP"]],
        "accounts": [
            {"seed": anchor_seed.hex(), "of": ids["AP"], "program": ids["AP"]},
            {"seed": sha(NULLIFIER_SEED_DOMAIN, nf).hex(), "of": BP},
            {"seed": CONFIG_SEED.hex(), "of": BP},
            {"account": ids["DEF_USDC"], "program": TP},
            {"actor": "account", "program": TP, "openings": True},
            {"seed": CONFIG_SEED.hex(), "of": BP, "program": "native"},
            {"actor": "account", "program": "native", "openings": True},
        ],
        "message": message.hex(),
        "openings": [opening.hex()],
        "fee_payer": "account",
        "declared": {"credits": [{"asset": ids["DEF_USDC"], "amount": str(denom)}], "max_fee": "20000000"},
    }
    fd = os.open(out, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as f:
        json.dump(call, f)
    alias = sha(b"/LEE/v0.3/AccountId/Blinded/\0\0\0\0", V, opening)
    print(json.dumps({"message_bytes": len(message), "root": root.hex()[:16],
                      "alias": b58encode(alias), "nullifier_seed_pda_of": BP, "file": out}))

main()
