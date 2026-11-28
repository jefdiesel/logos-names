#!/usr/bin/env python3
"""
Zero-dependency Python 3 helper for the LNS v8 + lez_nft v3 deploy kit.

Responsibilities:
  - Parse and validate `deploy/params.toml` (Python 3.11+ `tomllib` with a
    built-in fallback parser for Python 3.9/3.10).
  - Enforce the `confirmed_by_jef = true` gate and reject any accidental
    private keys, mnemonics, or wallet passwords inside `params.toml`.
  - Cross-check all `[immutable]` compile-time parameters against
    `repo/core/src/{lib,market,shortlist}.rs` and `repo/shortlist/root.hex`.
  - Compute LEZ v0.4 PDAs, `owner_tag`, `tiered_id`, `settings_address`,
    `record_address`, `page_address`, `nft_token_address`, and exact v8 Borsh
    wire encodings (`Message::Instruction(Instruction::Configure { ... })`).
  - Read and decode on-chain `Settings` (v8), `NameRecord` (v8), `NftRecord`
    (lez_nft v3), clock timestamp, and native balances directly via sequencer
    JSON-RPC (`getAccountView`).
  - Built-in `--self-test` verifying all crypto/PDA/Borsh encodings against
    the pinned vectors in `repo/SPEC.md` §10.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import struct
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Constants (must match `repo/core/src/` and `repo/SPEC.md`)
# ---------------------------------------------------------------------------

BASE58_ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
BASE58_INDEX = {c: i for i, c in enumerate(BASE58_ALPHABET)}

PDA_PREFIX = b"/LEE/v0.2/AccountId/PDA/".ljust(32, b"\x00")
NATIVE_TOKEN_PROGRAM_ID = b"\x00" * 32
NATIVE_TOKEN_B58 = "1" * 32
CLOCK_ACCOUNT_BYTES = b"/LEZ/ClockProgramAccount/0000001"
PROGRAM_LOADER_ACCOUNT_BYTES = b"/LEZ/ProgramLoaderAccount/000001"

NAME_SEED_DOMAIN = b"logos-names/v5/name/"
PAGE_SEED_DOMAIN = b"logos-names/v5/page/"
DEPOSIT_SEED_DOMAIN = b"logos-names/v5/deposit/"
SETTINGS_SEED_DOMAIN = b"logos-names/v5/config"
OWNER_TAG_DOMAIN = b"logos-names/v5/owner"
COMMITMENT_DOMAIN = b"logos-names/v5/commitment"
COMMITMENT_SEED_DOMAIN = b"logos-names/v5/commit/"
TIER_ID_DOMAIN = b"logos-names/v7/tier/"

NFT_TOKEN_ID_DOMAIN = b"logos-names/v8/nft-token/"
NFT_TOKEN_SEED_DOMAIN = b"lez-nft/v3/token/"
NFT_DEED_SEED_DOMAIN = b"lez-nft/v3/deed/"

DAY_MS = 24 * 60 * 60 * 1000
MAX_SEGMENT_DATA_LEN = 96 * 1024  # program_loader_core::MAX_SEGMENT_DATA_LEN

FORBIDDEN_SECRET_KEYS = {
    "private_key",
    "privkey",
    "secret_key",
    "mnemonic",
    "recovery_phrase",
    "seed_phrase",
    "wallet_password",
    "password",
    "passphrase",
}

TERM_NAMES = {"30d": 0, "1y": 1, "5y": 2, "permanent": 3}

# Values Jef must confirm (in params.toml `[confirmed_by_jef]`) before a live send. The
# compile-time ones are fixed by the guest image; nft_program is fixed by the first Configure.
LIVE_REQUIRED_CONFIRMATIONS = (
    "sale_fee_bps", "grace_days", "premium_days", "term_days", "public_wait_minutes",
    "private_wait_minutes", "shortlist_root", "deed_image", "upgradeable",
    "admin", "treasury", "nft_admin", "rpc_url", "upgrade_wallet_dir", "image_ids",
)
# Admin-adjustable after deploy: deploying unconfirmed defaults is allowed (with a warning).
ADJUSTABLE_CONFIRMATIONS = ("prices", "premium_start", "held_names", "claims")
LNS_COLLECTION_DOMAIN = b"logos-names/v8/collection/"
NFT_CONFIG_SEED_DOMAIN = b"lez-nft/v3/config"

# ---------------------------------------------------------------------------
# Minimal TOML parser (uses stdlib `tomllib` on Python 3.11+, fallback below)
# ---------------------------------------------------------------------------

try:
    import tomllib as _tomllib  # type: ignore[attr-defined]
except ImportError:
    _tomllib = None


def _strip_toml_comment(line: str) -> str:
    in_str = False
    quote = ""
    escaped = False
    out = []
    for ch in line:
        if in_str:
            out.append(ch)
            if escaped:
                escaped = False
            elif ch == "\\" and quote == '"':
                escaped = True
            elif ch == quote:
                in_str = False
        else:
            if ch in ('"', "'"):
                in_str = True
                quote = ch
                out.append(ch)
            elif ch == "#":
                break
            else:
                out.append(ch)
    return "".join(out).strip()


def _parse_toml_value(raw: str) -> Any:
    raw = raw.strip()
    if raw == "true":
        return True
    if raw == "false":
        return False
    if len(raw) >= 2 and raw[0] == '"' and raw[-1] == '"':
        return bytes(raw[1:-1], "utf-8").decode("unicode_escape")
    if len(raw) >= 2 and raw[0] == "'" and raw[-1] == "'":
        return raw[1:-1]
    if raw.startswith("[") and raw.endswith("]"):
        inner = raw[1:-1].strip()
        if not inner:
            return []
        items: list[str] = []
        buf: list[str] = []
        depth = 0
        in_str = False
        quote = ""
        for ch in inner:
            if in_str:
                buf.append(ch)
                if ch == quote:
                    in_str = False
            else:
                if ch in ('"', "'"):
                    in_str = True
                    quote = ch
                    buf.append(ch)
                elif ch == "[":
                    depth += 1
                    buf.append(ch)
                elif ch == "]":
                    depth -= 1
                    buf.append(ch)
                elif ch == "," and depth == 0:
                    part = "".join(buf).strip()
                    if part:
                        items.append(part)
                    buf = []
                else:
                    buf.append(ch)
        tail = "".join(buf).strip()
        if tail:
            items.append(tail)
        return [_parse_toml_value(item) for item in items]
    cleaned = raw.replace("_", "")
    if re.fullmatch(r"-?\d+", cleaned):
        return int(cleaned)
    raise ValueError(f"unsupported TOML value: {raw!r}")


def load_toml(path: Path) -> dict[str, Any]:
    raw_bytes = path.read_bytes()
    if _tomllib is not None:
        return _tomllib.loads(raw_bytes.decode("utf-8"))
    root: dict[str, Any] = {}
    current: dict[str, Any] = root
    for lineno, raw_line in enumerate(raw_bytes.decode("utf-8").splitlines(), 1):
        line = _strip_toml_comment(raw_line)
        if not line:
            continue
        if line.startswith("[[") and line.endswith("]]"):
            table_path = [p.strip() for p in line[2:-2].split(".")]
            target = root
            for part in table_path[:-1]:
                target = target.setdefault(part, {})
            arr = target.setdefault(table_path[-1], [])
            entry: dict[str, Any] = {}
            arr.append(entry)
            current = entry
        elif line.startswith("[") and line.endswith("]"):
            table_path = [p.strip() for p in line[1:-1].split(".")]
            target = root
            for part in table_path:
                target = target.setdefault(part, {})
            current = target
        elif "=" in line:
            k, v = line.split("=", 1)
            key = k.strip()
            current[key] = _parse_toml_value(v)
        else:
            raise ValueError(f"{path}:{lineno}: invalid TOML line: {raw_line!r}")
    return root


# ---------------------------------------------------------------------------
# Crypto, Base58, PDAs, and Borsh codecs
# ---------------------------------------------------------------------------


def sha256(*parts: bytes) -> bytes:
    h = hashlib.sha256()
    for p in parts:
        h.update(p)
    return h.digest()


def base58_encode(data: bytes) -> str:
    n = int.from_bytes(data, "big")
    chars: list[str] = []
    while n > 0:
        n, rem = divmod(n, 58)
        chars.append(BASE58_ALPHABET[rem])
    leading = 0
    for b in data:
        if b == 0:
            leading += 1
        else:
            break
    return "1" * leading + "".join(reversed(chars))


def base58_decode(text: str) -> bytes:
    n = 0
    for ch in text:
        if ch not in BASE58_INDEX:
            raise ValueError(f"invalid base58 character {ch!r} in {text!r}")
        n = n * 58 + BASE58_INDEX[ch]
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big") if n > 0 else b""
    leading = 0
    for ch in text:
        if ch == "1":
            leading += 1
        else:
            break
    return b"\x00" * leading + raw


def strip_account_prefix(text: str) -> tuple[str | None, str]:
    s = text.strip()
    if s.startswith("Public/"):
        return ("Public", s[7:])
    if s.startswith("Private/"):
        return ("Private", s[8:])
    return (None, s)


def parse_account_id(text: str, *, allow_zero: bool = False, require_prefix: str | None = None) -> bytes:
    prefix, bare = strip_account_prefix(text)
    if "/" in bare or any(ch.isspace() for ch in bare):
        raise ValueError(f"malformed account ID: {text!r}")
    if require_prefix is not None and prefix is not None and prefix != require_prefix:
        raise ValueError(f"expected {require_prefix}/ account ID, got {text!r}")
    raw = base58_decode(bare)
    if len(raw) != 32:
        raise ValueError(f"account ID must decode to 32 bytes (got {len(raw)}): {text!r}")
    if not allow_zero and raw == b"\x00" * 32:
        raise ValueError(f"all-zero account ID ({NATIVE_TOKEN_B58}) is not allowed here")
    letters = [c for c in bare if c.isalpha()]
    if len(letters) >= 8 and (all(c.islower() for c in letters) or all(c.isupper() for c in letters)):
        raise ValueError(f"account ID {text!r} is all one case (likely mangled when copied)")
    return raw


def pda(program: bytes, seed: bytes) -> bytes:
    assert len(program) == 32 and len(seed) == 32
    return sha256(PDA_PREFIX, program, seed)


def settings_seed() -> bytes:
    return sha256(SETTINGS_SEED_DOMAIN)


def settings_address(program: bytes) -> bytes:
    return pda(program, settings_seed())


def name_seed(slot_id: bytes) -> bytes:
    return sha256(NAME_SEED_DOMAIN, slot_id)


def page_seed(slot_id: bytes) -> bytes:
    return sha256(PAGE_SEED_DOMAIN, slot_id)


def record_address(program: bytes, slot_id: bytes) -> bytes:
    return pda(program, name_seed(slot_id))


def page_address(program: bytes, slot_id: bytes) -> bytes:
    return pda(program, page_seed(slot_id))


def owner_tag(slot_id: bytes, account: bytes) -> bytes:
    return sha256(OWNER_TAG_DOMAIN, slot_id, account)


def price_tier_for_len(length: int) -> int:
    if length <= 5:
        return 0
    if length <= 7:
        return 1
    return 2


def tiered_id(base_id: bytes, price_tier: int) -> bytes:
    if price_tier not in (0, 1, 2):
        raise ValueError(f"invalid price_tier: {price_tier}")
    return sha256(TIER_ID_DOMAIN, bytes([price_tier]), base_id)


def nft_token_seed(token_id: bytes) -> bytes:
    return sha256(NFT_TOKEN_SEED_DOMAIN, token_id)


def nft_token_address(nft_program: bytes, token_id: bytes) -> bytes:
    return pda(nft_program, nft_token_seed(token_id))


def encode_u128(val: int) -> bytes:
    if val < 0 or val >= (1 << 128):
        raise ValueError(f"u128 out of range: {val}")
    return val.to_bytes(16, "little")


def encode_u64(val: int) -> bytes:
    if val < 0 or val >= (1 << 64):
        raise ValueError(f"u64 out of range: {val}")
    return struct.pack("<Q", val)


def encode_configure_instruction(
    *,
    admin: bytes,
    treasury: bytes,
    prices: list[list[int]],
    mode: int,
    deposits: list[int],
    claims_root: bytes,
    claims_until: int,
    claims_program: bytes,
    premium_start: int,
    nft_program: bytes,
) -> bytes:
    """Encodes `names_core::Instruction::Configure` (without the leading 0x00 Message tag)."""
    if len(prices) != 3 or any(len(row) != 4 for row in prices):
        raise ValueError("prices must be a 3x4 matrix")
    if len(deposits) != 2:
        raise ValueError("deposits must have 2 elements")
    if mode not in (0, 1):
        raise ValueError(f"mode must be 0 (fee) or 1 (deposit), got {mode}")
    out = bytearray()
    out.append(0x07)  # Instruction::Configure
    out.extend(admin)
    out.extend(treasury)
    for row in prices:
        for cell in row:
            out.extend(encode_u128(cell))
    out.append(mode)
    for dep in deposits:
        out.extend(encode_u128(dep))
    out.extend(claims_root)
    out.extend(encode_u64(claims_until))
    out.extend(claims_program)
    out.extend(encode_u128(premium_start))
    out.extend(nft_program)
    return bytes(out)


def encode_configure_message(**kwargs: Any) -> bytes:
    """Encodes `names_core::Message::Instruction(Instruction::Configure { ... })` (411 bytes)."""
    return b"\x00" + encode_configure_instruction(**kwargs)


class BorshReader:
    def __init__(self, data: bytes) -> None:
        self.data = data
        self.pos = 0

    def take(self, n: int) -> bytes:
        if self.pos + n > len(self.data):
            raise ValueError(f"truncated borsh at offset {self.pos} (need {n}, have {len(self.data) - self.pos})")
        chunk = self.data[self.pos : self.pos + n]
        self.pos += n
        return chunk

    def u8(self) -> int:
        return self.take(1)[0]

    def bool(self) -> bool:
        b = self.u8()
        if b not in (0, 1):
            raise ValueError(f"invalid bool byte {b}")
        return b == 1

    def u16(self) -> int:
        return struct.unpack("<H", self.take(2))[0]

    def u32(self) -> int:
        return struct.unpack("<I", self.take(4))[0]

    def u64(self) -> int:
        return struct.unpack("<Q", self.take(8))[0]

    def u128(self) -> int:
        return int.from_bytes(self.take(16), "little")

    def fixed32(self) -> bytes:
        return self.take(32)

    def vec_bytes(self) -> bytes:
        return self.take(self.u32())

    def string(self) -> str:
        return self.vec_bytes().decode("utf-8")

    def remaining(self) -> int:
        return len(self.data) - self.pos


def decode_settings_v8(data: bytes) -> dict[str, Any]:
    r = BorshReader(data)
    version = r.u8()
    if version != 8:
        raise ValueError(f"expected Settings version 8, got {version}")
    admin = base58_encode(r.fixed32())
    treasury = base58_encode(r.fixed32())
    prices = [[r.u128() for _ in range(4)] for _ in range(3)]
    mode = r.u8()
    deposits = [r.u128(), r.u128()]
    claims_root = r.fixed32().hex()
    claims_until = r.u64()
    claims_program = base58_encode(r.fixed32())
    premium_start = r.u128()
    nft_program = base58_encode(r.fixed32())
    if r.remaining() != 0:
        raise ValueError(f"unexpected trailing bytes in Settings: {r.remaining()}")
    return {
        "version": version,
        "admin": admin,
        "treasury": treasury,
        "prices": prices,
        "mode": mode,
        "mode_name": "deposit" if mode == 1 else "fee",
        "deposits": deposits,
        "claims_root": claims_root,
        "claims_until": claims_until,
        "claims_program": claims_program,
        "premium_start": premium_start,
        "nft_program": nft_program,
    }


def decode_name_record_v8(data: bytes) -> dict[str, Any]:
    r = BorshReader(data)
    version = r.u8()
    if version != 8:
        raise ValueError(f"expected NameRecord version 8, got {version}")
    owner = r.fixed32().hex()
    expires = r.u64()
    sealed = r.vec_bytes()
    claimed = r.bool()
    price_tier = r.u8()
    generation = r.fixed32().hex()
    subname_sale = None
    if r.bool():
        subname_sale = {
            "price": r.u128(),
            "payout": base58_encode(r.fixed32()),
        }
    nft_token_id = r.fixed32().hex()
    nft_shielded = r.bool()
    listing = None
    if r.bool():
        l_price = r.u128()
        l_pay_to = base58_encode(r.fixed32())
        l_expires_at = r.u64() if r.bool() else None
        l_nonce = r.u64()
        l_in_market = r.bool()
        listing = {
            "price": l_price,
            "pay_to": l_pay_to,
            "expires_at": l_expires_at,
            "nonce": l_nonce,
            "in_market": l_in_market,
        }
    listing_nonce = r.u64()
    site_listed = r.u16() if r.bool() else None
    if r.remaining() != 0:
        raise ValueError(f"unexpected trailing bytes in NameRecord: {r.remaining()}")
    return {
        "version": version,
        "owner": owner,
        "expires": expires,
        "permanent": expires == (1 << 64) - 1,
        "sealed_len": len(sealed),
        "claimed": claimed,
        "price_tier": price_tier,
        "generation": generation,
        "subname_sale": subname_sale,
        "nft_token_id": nft_token_id,
        "nft_shielded": nft_shielded,
        "listing": listing,
        "listing_nonce": listing_nonce,
        "site_listed": site_listed,
    }


def decode_nft_record_v3(data: bytes) -> dict[str, Any]:
    """
    Decodes a `lez_nft` v3 `NftRecord` following `notes/NAMES-NFT.md` §5.2 and
    `tools/engine-test/src/crosscheck.rs`.

    Handles both `shielded: bool` and `shielded_deed: Option<AccountId>` as the
    penultimate field before `expires: u64` (see `neon-job/CHANGES.md` §3).
    """
    r = BorshReader(data)
    version = r.u8()
    token_id = r.fixed32().hex()
    owner = r.fixed32().hex()
    collection = None
    if r.bool():
        collection = {
            "id": r.fixed32().hex(),
            "edition": r.u32(),
        }
    content = r.vec_bytes()
    sealed = r.vec_bytes()
    linked = None
    if r.bool():
        linked = {
            "program": base58_encode(r.fixed32()),
            "authority_actor": base58_encode(r.fixed32()),
            "target": r.fixed32().hex(),
            "record_actor": base58_encode(r.fixed32()),
        }
    rem = r.remaining()
    if rem == 9:
        # Layout A: shielded: bool (1) + expires: u64 (8)
        shielded = r.bool()
        shielded_deed = None
        expires = r.u64()
    elif rem == 41:
        # Layout B: shielded_deed: Some(AccountId) (1+32) + expires: u64 (8)
        has_deed = r.bool()
        shielded = has_deed
        shielded_deed = base58_encode(r.fixed32()) if has_deed else None
        expires = r.u64()
    else:
        raise ValueError(
            f"unexpected trailing byte count ({rem}) in lez_nft v3 NftRecord; "
            f"check `../lez-nft/core` field order against deploy/lib.py"
        )
    return {
        "version": version,
        "token_id": token_id,
        "owner": owner,
        "collection": collection,
        "content_utf8": content.decode("utf-8", errors="replace"),
        "sealed_len": len(sealed),
        "linked": linked,
        "shielded": shielded,
        "shielded_deed": shielded_deed,
        "expires": expires,
    }


def sale_split(price: int) -> tuple[int, int]:
    fee = (price + 39) // 40
    return (price - fee, fee)


def rerelease_premium(premium_start: int, elapsed_ms: int) -> int:
    duration = 21 * DAY_MS
    if premium_start == 0 or elapsed_ms >= duration:
        return 0
    day = elapsed_ms // DAY_MS
    rem = elapsed_ms % DAY_MS
    hi = premium_start >> day
    lo = 0 if (day + 1 == 21) else (premium_start >> (day + 1))
    span = hi - lo
    return hi - (span * rem) // DAY_MS


# ---------------------------------------------------------------------------
# JSON-RPC helpers
# ---------------------------------------------------------------------------


def rpc_call(rpc_url: str, method: str, params: Any = None, timeout: int = 15) -> Any:
    payload = json.dumps(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": [] if params is None else params,
        }
    ).encode("utf-8")
    req = urllib.request.Request(rpc_url, data=payload, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = json.loads(resp.read().decode("utf-8"))
    except urllib.error.URLError as exc:
        raise RuntimeError(f"RPC {method} to {rpc_url} failed: {exc}") from exc
    if "error" in body and body["error"] is not None:
        raise RuntimeError(f"RPC {method} error: {body['error']}")
    return body.get("result")


def get_actor_state(rpc_url: str, account_b58: str, program_b58: str) -> bytes:
    res = rpc_call(
        rpc_url,
        "getAccountView",
        [{"account_id": account_b58, "program_account_id": program_b58}],
    )
    if not res:
        return b""
    data = res.get("data") or {}
    states = data.get("actor_states") or data.get("shards") or {}
    arr = states.get(program_b58)
    if not arr:
        return b""
    if isinstance(arr, str):
        import base64
        return base64.b64decode(arr)
    return bytes(arr)


def get_native_balance(rpc_url: str, account_b58: str) -> tuple[int, int]:
    """Returns (balance_u128, nonce) for a public account."""
    res = rpc_call(
        rpc_url,
        "getAccountView",
        [{"account_id": account_b58, "program_account_id": NATIVE_TOKEN_B58}],
    )
    if not res:
        return (0, 0)
    nonce = int(res.get("nonce", 0))
    data = res.get("data") or {}
    states = data.get("actor_states") or data.get("shards") or {}
    arr = states.get(NATIVE_TOKEN_B58)
    if not arr or len(arr) != 16:
        return (0, nonce)
    return (int.from_bytes(bytes(arr), "little"), nonce)


# ---------------------------------------------------------------------------
# Validation against params.toml and compiled Rust source
# ---------------------------------------------------------------------------


def _scan_forbidden_keys(obj: Any, path: str = "") -> list[str]:
    hits: list[str] = []
    if isinstance(obj, dict):
        for k, v in obj.items():
            full = f"{path}.{k}" if path else k
            if k.lower() in FORBIDDEN_SECRET_KEYS:
                hits.append(full)
            hits.extend(_scan_forbidden_keys(v, full))
    elif isinstance(obj, list):
        for i, item in enumerate(obj):
            hits.extend(_scan_forbidden_keys(item, f"{path}[{i}]"))
    return hits


def _extract_rust_const(text: str, pattern: str, label: str) -> str:
    m = re.search(pattern, text, re.MULTILINE | re.DOTALL)
    if not m:
        raise ValueError(f"could not locate {label} in Rust source")
    return m.group(1)


def check_compile_time_immutables(params: dict[str, Any], repo_dir: Path) -> list[str]:
    """Cross-checks `[immutable]` in params.toml against `repo/core/src/` and `shortlist/root.hex`."""
    errors: list[str] = []
    imm = params.get("immutable", {})

    lib_rs = (repo_dir / "core" / "src" / "lib.rs").read_text(encoding="utf-8")
    market_rs = (repo_dir / "core" / "src" / "market.rs").read_text(encoding="utf-8")
    shortlist_rs = (repo_dir / "core" / "src" / "shortlist.rs").read_text(encoding="utf-8")
    root_hex_file = (repo_dir / "shortlist" / "root.hex").read_text(encoding="utf-8").strip()

    compiled = {
        "record_version": int(_extract_rust_const(lib_rs, r"pub const VERSION:\s*u8\s*=\s*(\d+);", "VERSION")),
        "shortlist_root": _extract_rust_const(
            lib_rs, r'pub const SHORTLIST_ROOT:.*?hex_literal\(b"([0-9a-f]{64})"\)', "SHORTLIST_ROOT"
        ),
        "shortlist_depth": int(
            _extract_rust_const(shortlist_rs, r"pub const SHORTLIST_DEPTH:\s*usize\s*=\s*(\d+);", "SHORTLIST_DEPTH")
        ),
        "sale_fee_bps": int(
            _extract_rust_const(market_rs, r"pub const SALE_FEE_BPS:\s*u16\s*=\s*(\d+);", "SALE_FEE_BPS")
        ),
        "min_sale_price": int(
            _extract_rust_const(market_rs, r"pub const SALE_FEE_DIVISOR:\s*u128\s*=\s*(\d+);", "SALE_FEE_DIVISOR")
        ),
        "grace_days": int(_extract_rust_const(lib_rs, r"pub const GRACE_MS:\s*u64\s*=\s*(\d+)\s*\*\s*DAY_MS;", "GRACE_MS")),
        "premium_days": int(
            _extract_rust_const(market_rs, r"pub const PREMIUM_DAYS:\s*u64\s*=\s*(\d+);", "PREMIUM_DAYS")
        ),
        "public_wait_minutes": int(
            _extract_rust_const(
                lib_rs,
                r"pub const PUBLIC_MIN_COMMIT_AGE_MS:\s*u64\s*=\s*(\d+)\s*\*\s*60\s*\*\s*1000;",
                "PUBLIC_MIN_COMMIT_AGE_MS",
            )
        ),
        "private_wait_minutes": int(
            _extract_rust_const(
                lib_rs,
                r"pub const MIN_COMMIT_AGE_MS:\s*u64\s*=\s*(\d+)\s*\*\s*60\s*\*\s*1000;",
                "MIN_COMMIT_AGE_MS",
            )
        ),
        "commit_max_age_hours": int(
            _extract_rust_const(
                lib_rs,
                r"pub const MAX_COMMIT_AGE_MS:\s*u64\s*=\s*(\d+)\s*\*\s*60\s*\*\s*60\s*\*\s*1000;",
                "MAX_COMMIT_AGE_MS",
            )
        ),
    }

    if compiled["shortlist_root"] != root_hex_file:
        errors.append(
            f"shortlist/root.hex ({root_hex_file}) != core/src/lib.rs SHORTLIST_ROOT ({compiled['shortlist_root']})"
        )

    for key, want in compiled.items():
        got = imm.get(key)
        if got != want:
            errors.append(
                f"[immutable].{key} = {got!r} does NOT match compiled core constant {want!r}"
            )

    if imm.get("term_days") != [30, 365, 1825]:
        errors.append(
            f"[immutable].term_days = {imm.get('term_days')!r} does NOT match compiled TERM_LENGTHS_MS [30, 365, 1825]"
        )

    nft_rs = (repo_dir / "core" / "src" / "nft.rs").read_text(encoding="utf-8")
    deed = re.search(r'pub const DEFAULT_DEED_CONTENT: &\[u8\] =\s*br#"(.*?)"#;', nft_rs, re.DOTALL)
    if not deed:
        errors.append("could not locate DEFAULT_DEED_CONTENT in core/src/nft.rs")
    else:
        try:
            image = json.loads(deed.group(1)).get("image")
        except ValueError:
            image = None
        if imm.get("deed_image") != image:
            errors.append(f"[immutable].deed_image = {imm.get('deed_image')!r} does NOT match core/src/nft.rs image {image!r}")

    if imm.get("nft_program_binding_is_permanent") is not True:
        errors.append("[immutable].nft_program_binding_is_permanent must be true")

    return errors


def load_expected_image_ids(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = _strip_toml_comment(raw)
        if not line:
            continue
        if "=" not in line:
            raise ValueError(f"{path}:{lineno}: expected `key = value`, got {raw!r}")
        k, v = line.split("=", 1)
        out[k.strip()] = v.strip().strip('"').strip("'")
    return out


def validate_params(params: dict[str, Any], repo_dir: Path, *, dry_run: bool) -> dict[str, Any]:
    errors: list[str] = []
    warnings: list[str] = []

    secret_hits = _scan_forbidden_keys(params)
    if secret_hits:
        errors.append(
            "REFUSED: params.toml must NEVER contain keys, mnemonics, or passwords "
            f"(found forbidden field(s): {', '.join(secret_hits)})"
        )

    # Confirmation is per value (`[confirmed_by_jef]` table), not one global switch: Jef
    # confirmed some values on 2026-11-28 and not others. A live deploy needs every value
    # in LIVE_REQUIRED_CONFIRMATIONS confirmed; a rehearsal (local chain) needs none.
    confirmed_tbl = params.get("confirmed_by_jef", {})
    if not isinstance(confirmed_tbl, dict):
        errors.append("[confirmed_by_jef] must be a table of `value = true|false` (one per value)")
        confirmed_tbl = {}
    rehearsal = params.get("rehearsal") is True
    unconfirmed = [k for k in LIVE_REQUIRED_CONFIRMATIONS if confirmed_tbl.get(k) is not True]
    if unconfirmed and not rehearsal:
        msg = "not confirmed by Jef in [confirmed_by_jef]: " + ", ".join(unconfirmed)
        if dry_run:
            warnings.append(msg + " (dry run allowed; a live send will refuse)")
        else:
            errors.append("REFUSED: " + msg)
    unknown = [k for k in confirmed_tbl if k not in LIVE_REQUIRED_CONFIRMATIONS and k not in ADJUSTABLE_CONFIRMATIONS]
    if unknown:
        errors.append("[confirmed_by_jef] has unknown keys: " + ", ".join(unknown))
    for k in ADJUSTABLE_CONFIRMATIONS:
        if confirmed_tbl.get(k) is not True:
            warnings.append(f"{k} is not confirmed by Jef (admin-adjustable after deploy; deploying the default)")
    confirmed = not unconfirmed

    # Rehearsal and live are told apart by the file, and a rehearsal may only reach a local chain.
    rpc_url = str(params.get("network", {}).get("rpc_url", "")).strip()
    if rehearsal:
        if not re.match(r"^https?://(127\.0\.0\.1|localhost|\[::1\])(:\d+)?/?$", rpc_url):
            errors.append(f"REFUSED: a rehearsal params file may only point at a loopback RPC, not {rpc_url!r}")
    else:
        if not rpc_url or "PLACEHOLDER" in rpc_url:
            (warnings if dry_run else errors).append("[network].rpc_url is a placeholder (fill it on Monday)")
        if re.match(r"^https?://(127\.0\.0\.1|localhost|\[::1\])", rpc_url or "x"):
            errors.append("a live params file points at a loopback RPC: use a rehearsal file (rehearsal = true) for local chains")
    upgrade_dir = str(params.get("keys", {}).get("upgrade_wallet_dir", "")).strip()
    if not rehearsal and (not upgrade_dir or "PLACEHOLDER" in upgrade_dir):
        (warnings if dry_run else errors).append("[keys].upgrade_wallet_dir is a placeholder (the folder that will hold the header keys)")
    if params.get("upgradeable") is not True:
        errors.append("upgradeable must be true on testnet (Jef, 2026-11-28): the programs keep their header (upgrade) key")

    errors.extend(check_compile_time_immutables(params, repo_dir))

    # Accounts
    accounts = params.get("accounts", {})
    parsed_accounts: dict[str, str] = {}
    for field in ("deployer_payer", "nft_admin", "admin", "treasury"):
        val = str(accounts.get(field, "")).strip()
        bucket = warnings if dry_run else errors
        if not val or "PLACEHOLDER" in val:
            bucket.append(f"[accounts].{field} is not filled in yet")
            continue
        try:
            raw = parse_account_id(val, allow_zero=False, require_prefix="Public")
            parsed_accounts[field] = base58_encode(raw)
        except ValueError as exc:
            bucket.append(f"[accounts].{field}: {exc}")

    # Registry settings
    reg = params.get("registry", {})
    mode_str = str(reg.get("mode", "")).strip().lower()
    if mode_str not in ("fee", "deposit"):
        errors.append(f"[registry].mode must be 'fee' or 'deposit', got {mode_str!r}")
    mode_num = 1 if mode_str == "deposit" else 0

    deposits = reg.get("deposits", [0, 0])
    if not isinstance(deposits, list) or len(deposits) != 2 or any(not isinstance(x, int) or x < 0 for x in deposits):
        errors.append("[registry].deposits must be a 2-element list of non-negative integers")

    premium_start = reg.get("premium_start")
    if not isinstance(premium_start, int) or premium_start < 0:
        errors.append(f"[registry].premium_start must be a non-negative integer, got {premium_start!r}")

    prices_tbl = reg.get("prices", {})
    prices_matrix: list[list[int]] = []
    for row_key in ("tier_5", "tier_6_7", "tier_8_plus"):
        row = prices_tbl.get(row_key)
        if not isinstance(row, list) or len(row) != 4 or any(not isinstance(x, int) or x < 0 for x in row):
            errors.append(f"[registry.prices].{row_key} must be a 4-element list [30d, 1y, 5y, permanent]")
        else:
            prices_matrix.append(row)

    claims = reg.get("claims", {})
    claims_enabled = bool(claims.get("enabled", False))
    if claims_enabled:
        c_root_hex = str(claims.get("claims_root", "")).strip().lower()
        if not re.fullmatch(r"[0-9a-f]{64}", c_root_hex) or c_root_hex == "0" * 64:
            errors.append("[registry.claims].claims_root must be a non-zero 64-hex root when claims are enabled")
        c_until = claims.get("claims_until", 0)
        if not isinstance(c_until, int) or c_until <= 0:
            errors.append("[registry.claims].claims_until must be a positive Unix ms timestamp when claims are enabled")
        c_prog_str = str(claims.get("claims_program", "")).strip()
        try:
            c_prog_b58 = base58_encode(parse_account_id(c_prog_str, allow_zero=False))
        except ValueError as exc:
            errors.append(f"[registry.claims].claims_program: {exc}")
            c_prog_b58 = NATIVE_TOKEN_B58
    else:
        c_root_hex = "0" * 64
        c_until = 0
        c_prog_b58 = NATIVE_TOKEN_B58

    if (
        mode_num == 0
        and parsed_accounts.get("admin")
        and parsed_accounts.get("treasury")
        and parsed_accounts["admin"] != parsed_accounts["treasury"]
    ):
        warnings.append(
            "In fee mode with admin != treasury, admin pays tier registration fees to treasury "
            "when placing held names in post-deploy.sh (ensure admin is funded accordingly, or "
            "set admin == treasury initially and rotate treasury afterwards)."
        )

    # Held names check (non-blocking for deploy.sh if owner left empty, checked strictly in post-deploy.sh)
    held = params.get("held_names", [])
    if not isinstance(held, list):
        errors.append("[[held_names]] must be a list of tables")

    return {
        "ok": len(errors) == 0,
        "errors": errors,
        "warnings": warnings,
        "confirmed_by_jef": confirmed,
        "rehearsal": rehearsal,
        "upgradeable": params.get("upgradeable") is True,
        "deed_image": params.get("immutable", {}).get("deed_image"),
        "accounts": parsed_accounts,
        "raw_accounts": {k: v for k, v in accounts.items() if not isinstance(v, str)},
        "mode": mode_num,
        "mode_name": mode_str,
        "deposits": deposits,
        "premium_start": premium_start,
        "prices": prices_matrix,
        "claims": {
            "enabled": claims_enabled,
            "root": c_root_hex,
            "until": c_until,
            "program": c_prog_b58,
        },
        "immutable": params.get("immutable", {}),
        "network": params.get("network", {}),
        "artifacts": params.get("artifacts", {}),
        "tooling": params.get("tooling", {}),
        "held_names": held,
        "post_deploy": params.get("post_deploy", {}),
    }


# ---------------------------------------------------------------------------
# CLI subcommands for bash scripts
# ---------------------------------------------------------------------------


def cmd_validate(args: list[str]) -> int:
    params_path = Path(args[0])
    repo_dir = Path(args[1])
    dry_run = "--dry-run" in args
    params = load_toml(params_path)
    result = validate_params(params, repo_dir, dry_run=dry_run)
    print(json.dumps(result, indent=2))
    return 0 if result["ok"] else 1


def cmd_check_images(args: list[str]) -> int:
    expected_path = Path(args[0])
    reg_elf = Path(args[1])
    nft_elf = Path(args[2])
    reg_id = args[3].strip().lower()
    nft_id = args[4].strip().lower()
    expected = load_expected_image_ids(expected_path)

    errors: list[str] = []
    for label, got_id, key in (
        ("registry", reg_id, "registry_image_id"),
        ("nft", nft_id, "nft_image_id"),
    ):
        want = expected.get(key, "")
        if not want or "REPLACE_WITH" in want.upper():
            errors.append(f"{expected_path}: {key} is still a placeholder ({want!r})")
        elif not re.fullmatch(r"[0-9a-f]{64}", want):
            errors.append(f"{expected_path}: {key} is not 64 lowercase hex ({want!r})")
        elif got_id != want:
            errors.append(f"{label} image_id mismatch: built {got_id}, expected {want}")

    sha_map = {
        "registry_elf_sha256": hashlib.sha256(reg_elf.read_bytes()).hexdigest(),
        "nft_elf_sha256": hashlib.sha256(nft_elf.read_bytes()).hexdigest(),
    }
    for key, actual_sha in sha_map.items():
        if key in expected and expected[key] and "REPLACE_WITH" not in expected[key].upper():
            if actual_sha != expected[key].lower():
                errors.append(f"{key} mismatch: file {actual_sha}, expected {expected[key]}")

    out = {
        "ok": len(errors) == 0,
        "errors": errors,
        "registry": {
            "elf": str(reg_elf),
            "bytes": reg_elf.stat().st_size,
            "segments": (reg_elf.stat().st_size + MAX_SEGMENT_DATA_LEN - 1) // MAX_SEGMENT_DATA_LEN,
            "elf_sha256": sha_map["registry_elf_sha256"],
            "image_id": reg_id,
            "expected_image_id": expected.get("registry_image_id", ""),
        },
        "nft": {
            "elf": str(nft_elf),
            "bytes": nft_elf.stat().st_size,
            "segments": (nft_elf.stat().st_size + MAX_SEGMENT_DATA_LEN - 1) // MAX_SEGMENT_DATA_LEN,
            "elf_sha256": sha_map["nft_elf_sha256"],
            "image_id": nft_id,
            "expected_image_id": expected.get("nft_image_id", ""),
        },
    }
    print(json.dumps(out, indent=2))
    return 0 if out["ok"] else 1


def cmd_build_configure(args: list[str]) -> int:
    params_path = Path(args[0])
    repo_dir = Path(args[1])
    registry_b58 = args[2]
    nft_b58 = args[3]
    signer_b58 = args[4] if len(args) > 4 else registry_b58

    params = load_toml(params_path)
    v = validate_params(params, repo_dir, dry_run=True)
    if not v["ok"]:
        print(json.dumps({"ok": False, "errors": v["errors"]}, indent=2), file=sys.stderr)
        return 1

    reg_bytes = parse_account_id(registry_b58, allow_zero=False)
    nft_bytes = parse_account_id(nft_b58, allow_zero=False)
    if nft_bytes == NATIVE_TOKEN_PROGRAM_ID or nft_bytes == reg_bytes:
        raise ValueError("nft_program cannot be the native token program or the registry itself")

    admin_bytes = parse_account_id(v["accounts"]["admin"])
    treasury_bytes = parse_account_id(v["accounts"]["treasury"])
    claims_root = bytes.fromhex(v["claims"]["root"])
    claims_program = parse_account_id(v["claims"]["program"], allow_zero=True)
    signer_bytes = parse_account_id(signer_b58)

    msg_bytes = encode_configure_message(
        admin=admin_bytes,
        treasury=treasury_bytes,
        prices=v["prices"],
        mode=v["mode"],
        deposits=v["deposits"],
        claims_root=claims_root,
        claims_until=v["claims"]["until"],
        claims_program=claims_program,
        premium_start=v["premium_start"],
        nft_program=nft_bytes,
    )
    s_seed = settings_seed()
    s_addr = settings_address(reg_bytes)
    change = {
        "message": msg_bytes.hex(),
        "spent": "0",
        "mentions": [
            {
                "account": base58_encode(signer_bytes),
                "program": base58_encode(reg_bytes),
                "seed": None,
                "signs": True,
            },
            {
                "account": base58_encode(s_addr),
                "program": base58_encode(reg_bytes),
                "seed": s_seed.hex(),
                "signs": False,
            },
        ],
    }
    print(
        json.dumps(
            {
                "ok": True,
                "registry": base58_encode(reg_bytes),
                "nft_program": base58_encode(nft_bytes),
                "settings_pda": base58_encode(s_addr),
                "settings_seed": s_seed.hex(),
                "message_len": len(msg_bytes),
                "message_hex": msg_bytes.hex(),
                "module_send_json": change,
            },
            indent=2,
        )
    )
    return 0


def cmd_verify_settings(args: list[str]) -> int:
    rpc_url, registry_b58, nft_b58, params_path, repo_dir = (
        args[0],
        args[1],
        args[2],
        Path(args[3]),
        Path(args[4]),
    )
    params = load_toml(params_path)
    v = validate_params(params, repo_dir, dry_run=True)
    reg_bytes = parse_account_id(registry_b58)
    s_addr = base58_encode(settings_address(reg_bytes))
    raw = get_actor_state(rpc_url, s_addr, registry_b58)
    if not raw:
        print(json.dumps({"ok": False, "error": f"settings PDA {s_addr} is empty on chain"}, indent=2))
        return 1
    got = decode_settings_v8(raw)
    mismatches: list[str] = []
    checks = [
        ("version", got["version"], 8),
        ("admin", got["admin"], v["accounts"]["admin"]),
        ("treasury", got["treasury"], v["accounts"]["treasury"]),
        ("prices", got["prices"], v["prices"]),
        ("mode", got["mode"], v["mode"]),
        ("deposits", got["deposits"], v["deposits"]),
        ("claims_root", got["claims_root"], v["claims"]["root"]),
        ("claims_until", got["claims_until"], v["claims"]["until"]),
        ("claims_program", got["claims_program"], v["claims"]["program"]),
        ("premium_start", got["premium_start"], v["premium_start"]),
        ("nft_program", got["nft_program"], nft_b58),
    ]
    for field, actual, want in checks:
        if actual != want:
            mismatches.append(f"{field}: on-chain={actual!r}, expected={want!r}")
    print(json.dumps({"ok": not mismatches, "settings_pda": s_addr, "settings": got, "mismatches": mismatches}, indent=2))
    return 0 if not mismatches else 1


def cmd_verify_nft_link(args: list[str]) -> int:
    rpc_url, registry_b58, nft_b58, base_id_hex, name_len_str = args[0], args[1], args[2], args[3], args[4]
    expected_owner = args[5] if len(args) > 5 and args[5] else None
    reg_bytes = parse_account_id(registry_b58)
    nft_bytes = parse_account_id(nft_b58)
    base_id = bytes.fromhex(base_id_hex)
    tier = price_tier_for_len(int(name_len_str))
    slot_id = tiered_id(base_id, tier)
    rec_addr = base58_encode(record_address(reg_bytes, slot_id))

    rec_raw = get_actor_state(rpc_url, rec_addr, registry_b58)
    if not rec_raw:
        print(json.dumps({"ok": False, "error": f"NameRecord at {rec_addr} is empty"}, indent=2))
        return 1
    rec = decode_name_record_v8(rec_raw)

    token_id = bytes.fromhex(rec["nft_token_id"])
    tok_addr = base58_encode(nft_token_address(nft_bytes, token_id))
    tok_raw = get_actor_state(rpc_url, tok_addr, nft_b58)
    if not tok_raw:
        print(
            json.dumps(
                {
                    "ok": False,
                    "error": f"NftRecord at {tok_addr} is empty (mint failed or wrong nft_program)",
                    "record_address": rec_addr,
                    "record": rec,
                },
                indent=2,
            )
        )
        return 1
    tok = decode_nft_record_v3(tok_raw)

    errors: list[str] = []
    if expected_owner:
        want_tag = owner_tag(slot_id, parse_account_id(expected_owner)).hex()
        if rec["owner"] != want_tag:
            errors.append(f"NameRecord.owner {rec['owner']} != expected owner_tag {want_tag}")
    if tok["token_id"] != rec["nft_token_id"]:
        errors.append(f"token_id mismatch: nft={tok['token_id']} reg={rec['nft_token_id']}")
    if tok["owner"] != rec["owner"]:
        errors.append(f"owner mismatch: nft={tok['owner']} reg={rec['owner']}")
    if tok["expires"] != rec["expires"]:
        errors.append(f"expires mismatch: nft={tok['expires']} reg={rec['expires']}")
    if tok["shielded"] != rec["nft_shielded"]:
        errors.append(f"shielded mismatch: nft={tok['shielded']} reg={rec['nft_shielded']}")
    linked = tok.get("linked")
    if not linked:
        errors.append("NftRecord.linked is None")
    else:
        want_auth = base58_encode(settings_address(reg_bytes))
        if linked["program"] != registry_b58:
            errors.append(f"linked.program {linked['program']} != {registry_b58}")
        if linked["authority_actor"] != want_auth:
            errors.append(f"linked.authority_actor {linked['authority_actor']} != {want_auth}")
        if linked["target"] != slot_id.hex():
            errors.append(f"linked.target {linked['target']} != slot_id {slot_id.hex()}")
        if linked["record_actor"] != rec_addr:
            errors.append(f"linked.record_actor {linked['record_actor']} != {rec_addr}")

    print(
        json.dumps(
            {
                "ok": not errors,
                "slot_id": slot_id.hex(),
                "price_tier": tier,
                "record_address": rec_addr,
                "nft_token_address": tok_addr,
                "record": rec,
                "nft_record": tok,
                "errors": errors,
            },
            indent=2,
        )
    )
    return 0 if not errors else 1


def cmd_rpc_probe(args: list[str]) -> int:
    rpc_url = args[0]
    expected_channel = args[1].strip().lower() if len(args) > 1 else ""
    block = rpc_call(rpc_url, "getLastBlockId")
    channel = rpc_call(rpc_url, "getChannelId", {})
    ok = True
    errors: list[str] = []
    if expected_channel and str(channel).lower() != expected_channel:
        ok = False
        errors.append(f"channel_id mismatch: sequencer={channel}, expected={expected_channel}")
    print(json.dumps({"ok": ok, "rpc_url": rpc_url, "last_block_id": block, "channel_id": channel, "errors": errors}, indent=2))
    return 0 if ok else 1


def cmd_balance(args: list[str]) -> int:
    rpc_url, account = args[0], args[1]
    b58 = base58_encode(parse_account_id(account, allow_zero=False))
    bal, nonce = get_native_balance(rpc_url, b58)
    print(json.dumps({"account": b58, "balance": str(bal), "nonce": nonce, "admitted": bal > 0 or nonce > 0}))
    return 0


def cmd_verify_program(args: list[str]) -> int:
    rpc_url, program_b58 = args[0], args[1]
    prog_norm = base58_encode(parse_account_id(program_b58, allow_zero=False))
    loader_b58 = base58_encode(PROGRAM_LOADER_ACCOUNT_BYTES)
    raw = get_actor_state(rpc_url, prog_norm, loader_b58)
    out = {
        "ok": len(raw) > 0,
        "program": prog_norm,
        "loader_account": loader_b58,
        "header_bytes": len(raw),
    }
    print(json.dumps(out, indent=2))
    return 0 if out["ok"] else 1


def cmd_inspect_record(args: list[str]) -> int:
    rpc_url, registry_b58, base_id_hex, name_len_str = args[0], args[1], args[2], args[3]
    expected_owner = args[4] if len(args) > 4 and args[4] else None
    reg_bytes = parse_account_id(registry_b58, allow_zero=False)
    base_id = bytes.fromhex(base_id_hex)
    tier = price_tier_for_len(int(name_len_str))
    slot_id = tiered_id(base_id, tier)
    rec_addr = base58_encode(record_address(reg_bytes, slot_id))
    rec_raw = get_actor_state(rpc_url, rec_addr, base58_encode(reg_bytes))
    if not rec_raw:
        print(
            json.dumps(
                {
                    "exists": False,
                    "slot_id": slot_id.hex(),
                    "price_tier": tier,
                    "record_address": rec_addr,
                    "owner_matches": False,
                    "record": None,
                },
                indent=2,
            )
        )
        return 0
    rec = decode_name_record_v8(rec_raw)
    owner_matches: bool | None = None
    want_tag: str | None = None
    if expected_owner:
        want_tag = owner_tag(slot_id, parse_account_id(expected_owner, allow_zero=False)).hex()
        owner_matches = rec["owner"] == want_tag
    print(
        json.dumps(
            {
                "exists": True,
                "slot_id": slot_id.hex(),
                "price_tier": tier,
                "record_address": rec_addr,
                "expected_owner_tag": want_tag,
                "owner_matches": owner_matches,
                "record": rec,
            },
            indent=2,
        )
    )
    return 0


def cmd_id_info(args: list[str]) -> int:
    registry_b58, nft_b58 = args[0], args[1]
    reg_bytes = parse_account_id(registry_b58, allow_zero=False)
    nft_bytes = parse_account_id(nft_b58, allow_zero=False)
    s_seed = settings_seed()
    s_addr = settings_address(reg_bytes)
    print(
        json.dumps(
            {
                "registry_b58": base58_encode(reg_bytes),
                "registry_hex": reg_bytes.hex(),
                "settings_pda": base58_encode(s_addr),
                "settings_seed_hex": s_seed.hex(),
                "nft_b58": base58_encode(nft_bytes),
                "nft_hex": nft_bytes.hex(),
            },
            indent=2,
        )
    )
    return 0


def nft_config_address(nft_program: bytes) -> bytes:
    return pda(nft_program, sha256(NFT_CONFIG_SEED_DOMAIN))


def lns_collection_id(registry_program: bytes) -> bytes:
    return sha256(LNS_COLLECTION_DOMAIN, registry_program)


def cmd_ids(args: list[str]) -> int:
    """`ids <registry> <nft>`: the addresses and ids the deploy derives, as JSON."""
    reg = parse_account_id(args[0])
    nft = parse_account_id(args[1])
    print(json.dumps({
        "registry": base58_encode(reg),
        "nft_program": base58_encode(nft),
        "registry_settings": base58_encode(settings_address(reg)),
        "nft_config": base58_encode(nft_config_address(nft)),
        "lns_collection_id": lns_collection_id(reg).hex(),
    }, indent=2))
    return 0


def cmd_header_image(args: list[str]) -> int:
    """`header-image <rpc> <header>`: the image id a program header holds (66-byte tagged
    LoaderEntry::Header), or nothing when the account holds no program."""
    rpc, acct = args[0], args[1]
    loader = base58_encode(bytes([0xFE] * 32))
    st = get_actor_state(rpc, acct, loader)
    if len(st) == 66 and st[0] == 0:
        print(st[1:33].hex())
        return 0
    return 1


def cmd_self_test() -> int:
    # 1. SPEC.md §10 PDA vectors (program = 0x07 * 32)
    prog = b"\x07" * 32
    assert base58_encode(prog) == "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx"
    assert base58_encode(settings_address(prog)) == "2aXkMrPG771Scj8LWBMyUGjJRou8Ls9kotSbnBfvoVfV"
    id_seq = bytes(range(32))
    assert base58_encode(record_address(prog, id_seq)) == "7bwwvMLtxqqNYiXsd5bYaq8e31E1odYf6jAK5z7tmUdd"
    assert base58_encode(page_address(prog, id_seq)) == "8M4vmdxCXL4eS8QXiHSdbeUUPt5Vbx1UVjLcv2cPk9Y5"

    # 2. SPEC.md §10 owner_tag vector
    acct = parse_account_id("B1UN3hPgxacgHKBRoThcAmsPajGcUf6YXUhgB36x4DAd")
    assert owner_tag(id_seq, acct).hex() == "663f31bd3c64e9f17fbd399d3b6224f60b298e95dfca4ee8a4cf8db28b3c7a55"

    # 3. SPEC.md §10 Instruction::Configure vector (line 1053)
    cfg = encode_configure_instruction(
        admin=b"\x01" * 32,
        treasury=b"\x02" * 32,
        prices=[[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]],
        mode=0,
        deposits=[0, 0],
        claims_root=b"\x00" * 32,
        claims_until=0,
        claims_program=b"\x07" * 32,
        premium_start=4_000_000_000,
        nft_program=b"\x09" * 32,
    )
    want_cfg = (
        b"\x07"
        + b"\x01" * 32
        + b"\x02" * 32
        + b"".join(i.to_bytes(16, "little") for i in range(1, 13))
        + b"\x00"
        + b"\x00" * 72
        + b"\x07" * 32
        + bytes.fromhex("00286bee000000000000000000000000")
        + b"\x09" * 32
    )
    assert cfg == want_cfg, f"Configure mismatch:\n{cfg.hex()}\n!=\n{want_cfg.hex()}"
    assert len(b"\x00" + cfg) == 411

    # 4. Settings v8 decode round-trip (410 bytes = version 0x08 + cfg[1:])
    decoded_settings = decode_settings_v8(b"\x08" + cfg[1:])
    assert decoded_settings["version"] == 8
    assert decoded_settings["prices"] == [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]]
    assert decoded_settings["premium_start"] == 4_000_000_000
    assert decoded_settings["nft_program"] == base58_encode(b"\x09" * 32)

    # 5. SPEC.md §10 NameRecord v8 vector (line 1092)
    rec_hex = (
        "08"
        + "01" * 32
        + "ff" * 8
        + "01000000ee"
        + "00"
        + "00"
        + "99" * 32
        + "01"
        + "a0860100000000000000000000000000"
        + "02" * 32
        + "44" * 32
        + "00"
        + "01"
        + "80969800000000000000000000000000"
        + "03" * 32
        + "00"
        + "0100000000000000"
        + "01"
        + "0100000000000000"
        + "010000"
    )
    rec = decode_name_record_v8(bytes.fromhex(rec_hex))
    assert rec["version"] == 8 and rec["permanent"] is True
    assert rec["subname_sale"]["price"] == 100_000
    assert rec["nft_token_id"] == "44" * 32
    assert rec["listing"]["price"] == 10_000_000 and rec["listing"]["in_market"] is True
    assert rec["listing_nonce"] == 1 and rec["site_listed"] == 0

    # 6. Sale split & re-release premium math
    assert sale_split(10_000_000) == (9_750_000, 250_000)
    assert sale_split(40) == (39, 1)
    assert sale_split(3_000_017) == (3_000_017 - 75_001, 75_001)
    assert rerelease_premium(4_000_000_000, 0) == 4_000_000_000
    assert rerelease_premium(4_000_000_000, DAY_MS) == 2_000_000_000
    assert rerelease_premium(4_000_000_000, 21 * DAY_MS) == 0

    # 7. Repo params.toml check (dry-run mode)
    deploy_dir = Path(__file__).resolve().parent
    repo_dir = deploy_dir.parent
    example = load_toml(deploy_dir / "params.toml")
    imm_errs = check_compile_time_immutables(example, repo_dir)
    assert not imm_errs, f"params.toml immutables mismatch: {imm_errs}"

    print("ok   deploy/lib.py self-test passed (SPEC.md §10 vectors, PDAs, v8 Borsh, math, params.toml)")
    return 0


def main() -> int:
    if len(sys.argv) < 2:
        print(
            "usage: lib.py <self-test|validate|check-images|build-configure|verify-settings|"
            "verify-nft-link|verify-program|inspect-record|id-info|rpc-probe|balance> ...",
            file=sys.stderr,
        )
        return 2
    cmd = sys.argv[1]
    rest = sys.argv[2:]
    if cmd in ("--self-test", "self-test"):
        return cmd_self_test()
    if cmd == "validate":
        return cmd_validate(rest)
    if cmd == "check-images":
        return cmd_check_images(rest)
    if cmd == "build-configure":
        return cmd_build_configure(rest)
    if cmd == "verify-settings":
        return cmd_verify_settings(rest)
    if cmd == "verify-nft-link":
        return cmd_verify_nft_link(rest)
    if cmd == "verify-program":
        return cmd_verify_program(rest)
    if cmd == "inspect-record":
        return cmd_inspect_record(rest)
    if cmd == "id-info":
        return cmd_id_info(rest)
    if cmd == "rpc-probe":
        return cmd_rpc_probe(rest)
    if cmd == "balance":
        return cmd_balance(rest)
    if cmd == "ids":
        return cmd_ids(rest)
    if cmd == "header-image":
        return cmd_header_image(rest)
    print(f"unknown subcommand: {cmd}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
