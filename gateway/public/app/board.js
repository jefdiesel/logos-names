// Reads a name's posts from the LNS board (board/core/src/lib.rs), the way
// `names posts` does (cli/src/board.rs, posts_of): the name's author account
// says how many posts it has made, and each post's address follows from its
// number, so no block scan is needed.

import { base58Decode, base58Encode, readShard } from "./names.js";

const utf8 = new TextEncoder();
const AUTHOR_DOMAIN = "logos-board/v1/author/";
const POST_DOMAIN = "logos-board/v1/post/";

function pad32(text) {
  const bytes = new Uint8Array(32);
  bytes.set(utf8.encode(text));
  return bytes;
}
const PDA_PREFIX = pad32("/LEE/v0.2/AccountId/PDA/");

function concat(...parts) {
  const out = new Uint8Array(parts.reduce((total, part) => total + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

async function sha256(bytes) {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
}

function u64le(n) {
  const out = new Uint8Array(8);
  let value = BigInt(n);
  for (let i = 0; i < 8; i += 1) {
    out[i] = Number(value & 0xffn);
    value >>= 8n;
  }
  return out;
}

/** AccountId::for_public_pda(program, sha256(parts)). */
async function pda(program, ...parts) {
  const programBytes = base58Decode(program);
  if (programBytes.length !== 32) throw new Error("board program ID must be 32 bytes");
  const seed = await sha256(concat(...parts));
  return base58Encode(await sha256(concat(PDA_PREFIX, programBytes, seed)));
}

export const authorAddress = (board, id) => pda(board, utf8.encode(AUTHOR_DOMAIN), id);
export const postAddress = (board, id, n) => pda(board, utf8.encode(POST_DOMAIN), id, u64le(n));

class Reader {
  constructor(bytes) {
    this.bytes = bytes;
    this.offset = 0;
  }
  take(length) {
    if (this.offset + length > this.bytes.length) throw new Error("account is truncated");
    const slice = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    return slice;
  }
  u8() {
    return this.take(1)[0];
  }
  u32() {
    const [a, b, c, d] = this.take(4);
    return (a | (b << 8) | (c << 16) | (d << 24)) >>> 0;
  }
  u64() {
    const low = this.u32();
    const high = this.u32();
    return high * 2 ** 32 + low;
  }
  string() {
    return new TextDecoder("utf-8", { fatal: true }).decode(this.take(this.u32()));
  }
  postId() {
    return { author: this.take(32), n: this.u64() };
  }
}

export function decodeAuthor(bytes) {
  const r = new Reader(bytes);
  return { version: r.u8(), count: r.u64(), last: r.u64() };
}

export function decodePost(bytes) {
  const r = new Reader(bytes);
  const version = r.u8();
  const id = r.postId();
  const name = r.string();
  const parent = r.u8() === 0 ? null : r.postId();
  const time = r.u64();
  const text = r.string();
  const cids = [];
  for (let i = r.u32(); i > 0; i -= 1) cids.push(r.string());
  const hidden = r.u8() !== 0;
  return { version, id, name, parent, time, text, cids, hidden };
}

const sameBytes = (a, b) => a.length === b.length && a.every((byte, i) => byte === b[i]);

/** How many posts the name `id` has made. */
export async function postCount(rpcUrl, board, id) {
  const bytes = await readShard(rpcUrl, board, await authorAddress(board, id));
  return bytes.length === 0 ? 0 : decodeAuthor(bytes).count;
}

/**
 * Up to `count` posts by `name` (whose identifier is `id`), newest first,
 * numbered below `top`. Posts that are hidden, or whose name or author does not
 * match, are left out. Returns the posts and the number to continue below.
 */
export async function postsOf(rpcUrl, board, name, id, top, count = 10) {
  const numbers = [];
  for (let n = top - 1; n >= Math.max(0, top - count); n -= 1) numbers.push(n);
  const read = await Promise.all(
    numbers.map(async (n) => {
      const address = await postAddress(board, id, n);
      const bytes = await readShard(rpcUrl, board, address);
      if (bytes.length === 0) return null;
      try {
        return { ...decodePost(bytes), address };
      } catch {
        return null;
      }
    }),
  );
  const posts = read.filter(
    (post) => post && !post.hidden && post.name === name && sameBytes(post.id.author, id),
  );
  return { posts, next: Math.max(0, top - count) };
}
