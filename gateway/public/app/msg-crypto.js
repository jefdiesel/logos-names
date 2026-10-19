//#region node_modules/@noble/hashes/_u64.js
const fromNumH = (n) => n / 2 ** 32 | 0;
const fromNumL = (n) => n >>> 0;
function setU64FromNum(view, byteOffset, n, isLE) {
	const h = fromNumH(n);
	const l = fromNumL(n);
	view.setUint32(byteOffset, isLE ? l : h, isLE);
	view.setUint32(byteOffset + 4, isLE ? h : l, isLE);
}
//#endregion
//#region node_modules/@noble/hashes/utils.js
/**
* Checks if something is Uint8Array. Be careful: nodejs Buffer will return true.
* @param a - value to test
* @returns `true` when the value is a Uint8Array-compatible view.
* @example
* Check whether a value is a Uint8Array-compatible view.
* ```ts
* isBytes(new Uint8Array([1, 2, 3]));
* ```
*/
function isBytes$2(a) {
	return a instanceof Uint8Array || ArrayBuffer.isView(a) && a.constructor.name === "Uint8Array" && "BYTES_PER_ELEMENT" in a && a.BYTES_PER_ELEMENT === 1;
}
const atitle$2 = (title) => title ? `"${title}" ` : "";
/**
* Asserts something is a non-negative integer.
* @param n - number to validate
* @param title - label included in thrown errors
* @returns The validated number.
* @throws On wrong argument types. {@link TypeError}
* @throws On wrong argument ranges or values. {@link RangeError}
* @example
* Validate a non-negative integer option.
* ```ts
* anumber(32, 'length');
* ```
*/
function anumber$2(n, title = "") {
	if (typeof n !== "number") throw new TypeError(atitle$2(title) + "expected number, got " + typeof n);
	if (!Number.isSafeInteger(n) || n < 0) throw new RangeError(atitle$2(title) + "expected integer >= 0, got " + n);
	return n;
}
/**
* Asserts something is Uint8Array.
* @param value - value to validate
* @param length - optional exact length constraint
* @param title - label included in thrown errors
* @returns The validated byte array.
* @throws On wrong argument types. {@link TypeError}
* @throws On wrong argument ranges or values. {@link RangeError}
* @example
* Validate that a value is a byte array.
* ```ts
* abytes(new Uint8Array([1, 2, 3]));
* ```
*/
function abytes$2(value, length, title = "") {
	if (isBytes$2(value) && (length === void 0 || value.length === length)) return value;
	if (length !== void 0) anumber$2(length, "length");
	const bytes = isBytes$2(value);
	const ofLen = length !== void 0 ? ` of length ${length}` : "";
	const got = bytes ? `length=${value.length}` : `type=${typeof value}`;
	const message = atitle$2(title) + "expected Uint8Array" + ofLen + ", got " + got;
	if (!bytes) throw new TypeError(message);
	throw new RangeError(message);
}
/**
* Asserts something is a wrapped hash constructor.
* @param h - hash constructor to validate
* @throws On wrong argument types or invalid hash wrapper shape. {@link TypeError}
* @throws On invalid hash metadata ranges or values. {@link RangeError}
* @throws If the hash metadata allows empty outputs or block sizes. {@link Error}
* @example
* Validate a callable hash wrapper.
* ```ts
* import { ahash } from '@noble/hashes/utils.js';
* import { sha256 } from '@noble/hashes/sha2.js';
* ahash(sha256);
* ```
*/
function ahash(h) {
	if (typeof h !== "function" || typeof h.create !== "function") throw new TypeError("expected hash wrapped by utils.createHasher");
	anumber$2(h.outputLen);
	anumber$2(h.blockLen);
	if (h.outputLen < 1 || h.blockLen < 1) throw new Error("hash blockLen / outputLen must be >= 1");
}
const aobject$2 = (value, label) => {
	if (value === null || typeof value !== "object" || Array.isArray(value)) throw new TypeError((label === "object" ? "" : `"${label}" `) + "expected object, got type=" + typeof value);
};
const aopts = (value, label) => {
	aobject$2(value, label);
	const proto = Object.getPrototypeOf(value);
	if (proto !== Object.prototype && proto !== null) throw new TypeError(`"${label}" expected plain object`);
	if (Object.hasOwn(value, "__proto__")) throw new TypeError(`"${label}.__proto__" is not allowed`);
};
/**
* Asserts a hash instance has not been destroyed or finished.
* @param instance - hash instance to validate
* @param checkFinished - whether to reject finalized instances
* @throws If the hash instance has already been destroyed or finalized. {@link Error}
* @example
* Validate that a hash instance is still usable.
* ```ts
* import { aexists } from '@noble/hashes/utils.js';
* import { sha256 } from '@noble/hashes/sha2.js';
* const hash = sha256.create();
* aexists(hash);
* ```
*/
function aexists$1(instance, checkFinished = true) {
	if (instance.destroyed) throw new Error("hash was destroyed");
	if (checkFinished && instance.finished) throw new Error("digest() was already called");
}
/**
* Asserts output is a sufficiently-sized byte array.
* @param out - destination buffer
* @param instance - hash instance providing output length
* Oversized buffers are allowed; downstream code only promises to fill the first `outputLen` bytes.
* @throws On wrong argument types. {@link TypeError}
* @throws On wrong argument ranges or values. {@link RangeError}
* @example
* Validate a caller-provided digest buffer.
* ```ts
* import { aoutput } from '@noble/hashes/utils.js';
* import { sha256 } from '@noble/hashes/sha2.js';
* const hash = sha256.create();
* aoutput(new Uint8Array(hash.outputLen), hash);
* ```
*/
function aoutput$1(out, instance) {
	abytes$2(out, void 0, "output");
	const min = instance.outputLen;
	if (!(out.length >= min)) throw new RangeError("\"output\" expected length >= " + min);
}
/**
* Zeroizes typed arrays in place. Warning: JS provides no guarantees.
* @param arrays - arrays to overwrite with zeros
* @example
* Zeroize sensitive buffers in place.
* ```ts
* clean(new Uint8Array([1, 2, 3]));
* ```
*/
function clean$1(...arrays) {
	for (let i = 0; i < arrays.length; i++) arrays[i].fill(0);
}
/**
* Creates a DataView for byte-level manipulation.
* @param arr - source typed array
* @returns DataView over the same buffer region.
* @example
* Create a DataView over an existing buffer.
* ```ts
* createView(new Uint8Array(4));
* ```
*/
function createView$1(arr) {
	return new DataView(arr.buffer, arr.byteOffset, arr.byteLength);
}
/**
* Rotate-right operation for uint32 values.
* @param word - source word
* @param shift - shift amount in bits
* @returns Rotated word.
* @example
* Rotate a 32-bit word to the right.
* ```ts
* rotr(0x12345678, 8);
* ```
*/
function rotr(word, shift) {
	return word << 32 - shift | word >>> shift;
}
const hasHexBuiltin = /* @__PURE__ */ (() => typeof Uint8Array.from([]).toHex === "function" && typeof Uint8Array.fromHex === "function")();
const hexes = /* @__PURE__ */ Array.from({ length: 256 }, (_, i) => i.toString(16).padStart(2, "0"));
/**
* Convert byte array to hex string.
* Uses the built-in function when available and assumes it matches the tested
* fallback semantics.
* @param bytes - bytes to encode
* @returns Lowercase hexadecimal string.
* @throws On wrong argument types. {@link TypeError}
* @example
* Convert bytes to lowercase hexadecimal.
* ```ts
* bytesToHex(Uint8Array.from([0xca, 0xfe, 0x01, 0x23])); // 'cafe0123'
* ```
*/
function bytesToHex$1(bytes) {
	abytes$2(bytes);
	if (hasHexBuiltin) return bytes.toHex();
	let hex = "";
	for (let i = 0; i < bytes.length; i++) hex += hexes[bytes[i]];
	return hex;
}
function asciiToBase16(ch) {
	return ch >= 48 && ch <= 57 ? ch - 48 : ch >= 65 && ch <= 70 ? ch - 55 : ch >= 97 && ch <= 102 ? ch - 87 : void 0;
}
/**
* Convert hex string to byte array. Uses built-in function, when available.
* @param hex - hexadecimal string to decode
* @returns Decoded bytes.
* @throws On wrong argument types. {@link TypeError}
* @throws On wrong argument ranges or values. {@link RangeError}
* @example
* Decode lowercase hexadecimal into bytes.
* ```ts
* hexToBytes('cafe0123'); // Uint8Array.from([0xca, 0xfe, 0x01, 0x23])
* ```
*/
function hexToBytes$1(hex) {
	if (typeof hex !== "string") throw new TypeError("hex string expected, got " + typeof hex);
	if (hasHexBuiltin) try {
		return Uint8Array.fromHex(hex);
	} catch (error) {
		if (error instanceof SyntaxError) throw new RangeError(error.message);
		throw error;
	}
	const hl = hex.length;
	const al = hl / 2;
	if (hl % 2) throw new RangeError("hex string expected, got unpadded hex of length " + hl);
	const array = new Uint8Array(al);
	for (let ai = 0, hi = 0; ai < al; ai++, hi += 2) {
		const n1 = asciiToBase16(hex.charCodeAt(hi));
		const n2 = asciiToBase16(hex.charCodeAt(hi + 1));
		if (n1 === void 0 || n2 === void 0) {
			const char = hex[hi] + hex[hi + 1];
			throw new RangeError("hex string expected, got non-hex character \"" + char + "\" at index " + hi);
		}
		array[ai] = n1 * 16 + n2;
	}
	return array;
}
/**
* Merges default options and passed options.
* @param defaults - base option object
* @param opts - user overrides
* @param title - label included in thrown override errors
* @returns Fresh merged option object with a null prototype.
* @throws On wrong argument types. {@link TypeError}
* @example
* Merge user overrides onto default options.
* ```ts
* checkOpts({ dkLen: 32 }, { asyncTick: 10 });
* ```
*/
function checkOpts$1(defaults, opts, title = "opts") {
	aopts(defaults, "defaults");
	if (opts !== void 0) aopts(opts, title);
	return Object.assign(Object.create(null), defaults, opts);
}
/**
* Creates a callable hash function from a stateful class constructor.
* @param hashCons - hash constructor or factory
* @param info - optional metadata such as DER OID
* @returns Frozen callable hash wrapper with `.create()`.
*   Wrapper construction eagerly calls `hashCons(undefined)` once to read
*   `outputLen` / `blockLen`, so constructor side effects happen at module
*   init time.
* @throws On wrong argument types. {@link TypeError}
* @example
* Wrap a stateful hash constructor into a callable helper.
* ```ts
* import { createHasher } from '@noble/hashes/utils.js';
* import { sha256 } from '@noble/hashes/sha2.js';
* const wrapped = createHasher(sha256.create, { oid: sha256.oid });
* wrapped(new Uint8Array([1]));
* ```
*/
function createHasher(hashCons, info = {}) {
	if (typeof hashCons !== "function") throw new TypeError("\"hashCons\" expected function, got type=" + typeof hashCons);
	info = checkOpts$1({}, info, "info");
	const hashC = (msg, opts) => hashCons(opts).update(msg).digest();
	const tmp = hashCons(void 0);
	hashC.outputLen = tmp.outputLen;
	hashC.blockLen = tmp.blockLen;
	hashC.canXOF = tmp.canXOF;
	hashC.create = (opts) => hashCons(opts);
	Object.assign(hashC, info);
	return Object.freeze(hashC);
}
/**
* Cryptographically secure PRNG backed by `crypto.getRandomValues`.
* @param bytesLength - number of random bytes to generate
* @returns Random bytes.
* The platform `getRandomValues()` implementation still defines any
* single-call length cap, and this helper rejects oversize requests
* with a stable library `RangeError` instead of host-specific errors.
* @throws On wrong argument types. {@link TypeError}
* @throws On wrong argument ranges or values. {@link RangeError}
* @throws If the current runtime does not provide `crypto.getRandomValues`. {@link Error}
* @example
* Generate a fresh random key or nonce.
* ```ts
* const key = randomBytes(16);
* ```
*/
function randomBytes$1(bytesLength = 32) {
	anumber$2(bytesLength, "bytesLength");
	const cr = typeof globalThis === "object" ? globalThis.crypto : null;
	if (typeof cr?.getRandomValues !== "function") throw new Error("crypto.getRandomValues must be defined");
	if (bytesLength > 65536) throw new RangeError(`"bytesLength" expected <= 65536, got ${bytesLength}`);
	return cr.getRandomValues(new Uint8Array(bytesLength));
}
/**
* Creates OID metadata for NIST hashes with prefix `06 09 60 86 48 01 65 03 04 02`.
* @param suffix - final OID byte for the selected hash.
*   The helper accepts any byte even though only the documented NIST hash
*   suffixes are meaningful downstream.
* @returns Object containing the DER-encoded OID.
* @example
* Build OID metadata for a NIST hash.
* ```ts
* oidNist(0x01);
* ```
*/
const oidNist = (suffix) => ({ oid: Uint8Array.from([
	6,
	9,
	96,
	134,
	72,
	1,
	101,
	3,
	4,
	2,
	suffix
]) });
//#endregion
//#region node_modules/@noble/hashes/_md.js
/**
* Internal Merkle-Damgard hash utils.
* @module
*/
/**
* Shared 32-bit conditional boolean primitive reused by SHA-256, SHA-1, and MD5 `F`.
* Returns bits from `b` when `a` is set, otherwise from `c`.
* The XOR form is equivalent to MD5's `F(X,Y,Z) = XY v not(X)Z` because the masked terms never
* set the same bit.
* @param a - selector word
* @param b - word chosen when selector bit is set
* @param c - word chosen when selector bit is clear
* @returns Mixed 32-bit word.
* @example
* Combine three words with the shared 32-bit choice primitive.
* ```ts
* Chi(0xffffffff, 0x12345678, 0x87654321);
* ```
*/
function Chi(a, b, c) {
	return a & b ^ ~a & c;
}
/**
* Shared 32-bit majority primitive reused by SHA-256 and SHA-1.
* Returns bits shared by at least two inputs.
* @param a - first input word
* @param b - second input word
* @param c - third input word
* @returns Mixed 32-bit word.
* @example
* Combine three words with the shared 32-bit majority primitive.
* ```ts
* Maj(0xffffffff, 0x12345678, 0x87654321);
* ```
*/
function Maj(a, b, c) {
	return a & b ^ a & c ^ b & c;
}
/**
* Merkle-Damgard hash construction base class.
* Could be used to create MD5, RIPEMD, SHA1, SHA2.
* Accepts only byte-aligned `Uint8Array` input, even when the underlying spec describes bit
* strings with partial-byte tails.
* @param blockLen - internal block size in bytes
* @param outputLen - digest size in bytes
* @param padOffset - trailing length field size in bytes
* @param isLE - whether length and state words are encoded in little-endian
* @example
* Use a concrete subclass to get the shared Merkle-Damgard update/digest flow.
* ```ts
* import { _SHA1 } from '@noble/hashes/legacy.js';
* const hash = new _SHA1();
* hash.update(new Uint8Array([97, 98, 99]));
* hash.digest();
* ```
*/
var HashMD = class {
	blockLen;
	outputLen;
	canXOF = false;
	padOffset;
	isLE;
	buffer;
	view;
	finished = false;
	length = 0;
	pos = 0;
	destroyed = false;
	constructor(blockLen, outputLen, padOffset, isLE) {
		this.blockLen = blockLen;
		this.outputLen = outputLen;
		this.padOffset = padOffset;
		this.isLE = isLE;
		this.buffer = new Uint8Array(blockLen);
		this.view = createView$1(this.buffer);
	}
	update(data) {
		aexists$1(this);
		abytes$2(data);
		const { view, buffer, blockLen } = this;
		const len = data.length;
		let processed = false;
		for (let pos = 0; pos < len;) {
			const take = Math.min(blockLen - this.pos, len - pos);
			if (take === blockLen) {
				const dataView = createView$1(data);
				for (; blockLen <= len - pos; pos += blockLen) this.process(dataView, pos);
				processed = true;
				continue;
			}
			buffer.set(pos === 0 && take === len ? data : data.subarray(pos, pos + take), this.pos);
			this.pos += take;
			pos += take;
			if (this.pos === blockLen) {
				this.process(view, 0);
				this.pos = 0;
				processed = true;
			}
		}
		this.length += data.length;
		if (processed) this.roundClean();
		return this;
	}
	digestInto(out) {
		aexists$1(this);
		aoutput$1(out, this);
		this.finished = true;
		const { buffer, view, blockLen, isLE } = this;
		let { pos } = this;
		buffer[pos++] = 128;
		buffer.fill(0, pos);
		if (this.padOffset > blockLen - pos) {
			this.process(view, 0);
			buffer.fill(0);
		}
		setU64FromNum(view, blockLen - 8, this.length * 8, isLE);
		this.process(view, 0);
		this.roundClean();
		const oview = out === buffer ? view : createView$1(out);
		const len = this.outputLen;
		const outLen = len / 4;
		const state = this.get();
		if (len % 4 || outLen > state.length) throw new Error("invalid outputLen");
		for (let i = 0; i < outLen; i++) oview.setUint32(4 * i, state[i], isLE);
	}
	digest() {
		const { buffer, outputLen } = this;
		this.digestInto(buffer);
		const res = buffer.slice(0, outputLen);
		this.destroy();
		return res;
	}
	_cloneIntoMeta(to) {
		const { buffer, length, finished, destroyed, pos } = this;
		to.destroyed = destroyed;
		to.finished = finished;
		to.length = length;
		to.pos = pos;
		if (pos) to.buffer.set(buffer);
		return to;
	}
	clone() {
		return this._cloneInto();
	}
};
/**
* Initial SHA-2 state: fractional parts of square roots of first 16 primes 2..53.
* Check out `test/misc/sha2-gen-iv.js` for recomputation guide.
*/
/** Initial SHA256 state from RFC 6234 §6.1: the first 32 bits of the fractional parts of the
* square roots of the first eight prime numbers. Exported as a shared table; callers must treat
* it as read-only because constructors copy words from it by index. */
const SHA256_IV = /* @__PURE__ */ Uint32Array.from([
	1779033703,
	3144134277,
	1013904242,
	2773480762,
	1359893119,
	2600822924,
	528734635,
	1541459225
]);
//#endregion
//#region node_modules/@noble/hashes/sha2.js
/**
* SHA2 hash function. A.k.a. sha256, sha384, sha512, sha512_224, sha512_256.
* SHA256 is the fastest hash implementable in JS, even faster than Blake3.
* Check out {@link https://www.rfc-editor.org/rfc/rfc4634 | RFC 4634} and
* {@link https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf | FIPS 180-4}.
* @module
*/
/**
* SHA-224 / SHA-256 round constants from RFC 6234 §5.1: the first 32 bits
* of the cube roots of the first 64 primes (2..311).
*/
const SHA256_K = /* @__PURE__ */ Uint32Array.from([
	1116352408,
	1899447441,
	3049323471,
	3921009573,
	961987163,
	1508970993,
	2453635748,
	2870763221,
	3624381080,
	310598401,
	607225278,
	1426881987,
	1925078388,
	2162078206,
	2614888103,
	3248222580,
	3835390401,
	4022224774,
	264347078,
	604807628,
	770255983,
	1249150122,
	1555081692,
	1996064986,
	2554220882,
	2821834349,
	2952996808,
	3210313671,
	3336571891,
	3584528711,
	113926993,
	338241895,
	666307205,
	773529912,
	1294757372,
	1396182291,
	1695183700,
	1986661051,
	2177026350,
	2456956037,
	2730485921,
	2820302411,
	3259730800,
	3345764771,
	3516065817,
	3600352804,
	4094571909,
	275423344,
	430227734,
	506948616,
	659060556,
	883997877,
	958139571,
	1322822218,
	1537002063,
	1747873779,
	1955562222,
	2024104815,
	2227730452,
	2361852424,
	2428436474,
	2756734187,
	3204031479,
	3329325298
]);
/** Reusable SHA-224 / SHA-256 message schedule buffer `W_t` from RFC 6234 §6.2 step 1. */
const SHA256_W = /* @__PURE__ */ new Uint32Array(64);
/** Internal SHA-224 / SHA-256 compression engine from RFC 6234 §6.2. */
var SHA2_32B = class extends HashMD {
	A = 0;
	B = 0;
	C = 0;
	D = 0;
	E = 0;
	F = 0;
	G = 0;
	H = 0;
	constructor(outputLen, IV) {
		super(64, outputLen, 8, false);
		this.A = IV[0] | 0;
		this.B = IV[1] | 0;
		this.C = IV[2] | 0;
		this.D = IV[3] | 0;
		this.E = IV[4] | 0;
		this.F = IV[5] | 0;
		this.G = IV[6] | 0;
		this.H = IV[7] | 0;
	}
	get() {
		const { A, B, C, D, E, F, G, H } = this;
		return [
			A,
			B,
			C,
			D,
			E,
			F,
			G,
			H
		];
	}
	set(A, B, C, D, E, F, G, H) {
		this.A = A | 0;
		this.B = B | 0;
		this.C = C | 0;
		this.D = D | 0;
		this.E = E | 0;
		this.F = F | 0;
		this.G = G | 0;
		this.H = H | 0;
	}
	_cloneInto(to) {
		(to ||= new this.constructor()).set(...this.get());
		return this._cloneIntoMeta(to);
	}
	process(view, offset) {
		for (let i = 0; i < 16; i++, offset += 4) SHA256_W[i] = view.getUint32(offset, false);
		for (let i = 16; i < 64; i++) {
			const W15 = SHA256_W[i - 15];
			const W2 = SHA256_W[i - 2];
			const s0 = rotr(W15, 7) ^ rotr(W15, 18) ^ W15 >>> 3;
			const s1 = rotr(W2, 17) ^ rotr(W2, 19) ^ W2 >>> 10;
			SHA256_W[i] = s1 + SHA256_W[i - 7] + s0 + SHA256_W[i - 16] | 0;
		}
		let { A, B, C, D, E, F, G, H } = this;
		for (let i = 0; i < 64; i++) {
			const sigma1 = rotr(E, 6) ^ rotr(E, 11) ^ rotr(E, 25);
			const T1 = H + sigma1 + Chi(E, F, G) + SHA256_K[i] + SHA256_W[i] | 0;
			const T2 = (rotr(A, 2) ^ rotr(A, 13) ^ rotr(A, 22)) + Maj(A, B, C) | 0;
			H = G;
			G = F;
			F = E;
			E = D + T1 | 0;
			D = C;
			C = B;
			B = A;
			A = T1 + T2 | 0;
		}
		A = A + this.A | 0;
		B = B + this.B | 0;
		C = C + this.C | 0;
		D = D + this.D | 0;
		E = E + this.E | 0;
		F = F + this.F | 0;
		G = G + this.G | 0;
		H = H + this.H | 0;
		this.set(A, B, C, D, E, F, G, H);
	}
	roundClean() {
		clean$1(SHA256_W);
	}
	destroy() {
		this.destroyed = true;
		this.set(0, 0, 0, 0, 0, 0, 0, 0);
		clean$1(this.buffer);
	}
};
/** Internal SHA-256 hash class grounded in RFC 6234 §6.2. */
var _SHA256 = class extends SHA2_32B {
	constructor() {
		super(32, SHA256_IV);
	}
};
/**
* SHA2-256 hash function from RFC 4634. In JS it's the fastest: even faster than Blake3. Some info:
*
* - Trying 2^128 hashes would get 50% chance of collision, using birthday attack.
* - BTC network is doing 2^70 hashes/sec (2^95 hashes/year) as per 2025.
* - Each sha256 hash is executing 2^18 bit operations.
* - Good 2024 ASICs can do 200Th/sec with 3500 watts of power, corresponding to 2^36 hashes/joule.
* @param msg - message bytes to hash
* @param opts - Reserved hash options.
* @returns Digest bytes.
* @example
* Hash a message with SHA2-256.
* ```ts
* sha256(new Uint8Array([97, 98, 99]));
* ```
*/
const sha256 = /* @__PURE__ */ createHasher(() => new _SHA256(), /* @__PURE__ */ oidNist(1));
//#endregion
//#region node_modules/@noble/curves/utils.js
/**
* Hex, bytes and number utilities.
* @module
*/
/*! noble-curves - MIT License (c) 2022 Paul Miller (paulmillr.com) */
/**
* Validates that a value is an array, optionally validating each element.
* @param item - Value to validate.
* @param title - Label included in thrown errors.
* @param inner - Optional per-element validator, called with the element and its label.
* @returns The validated array.
* @example
* Validate an array of points before batch processing.
*
* ```ts
* aarray([1n, 2n], 'scalars');
* ```
*/
function aarray(item, title, inner = () => {}) {
	if (!Array.isArray(item)) throw new TypeError(`"${title}" expected array, got type=${typeof item}`);
	for (let i = 0; i < item.length; i++) inner(item[i], `${title}[${i}]`);
	return item;
}
/**
* Validates that a value is a byte array.
* @param value - Value to validate.
* @param length - Optional exact byte length.
* @param title - Optional field name.
* @returns Original byte array.
* @example
* Reject non-byte input before passing data into curve code.
*
* ```ts
* abytes(new Uint8Array(1));
* ```
*/
const abytes$1 = (value, length, title) => abytes$2(value, length, title);
/**
* Validates that a value is a non-negative safe integer.
* @param n - Value to validate.
* @param title - Optional field name.
* @returns The validated number.
* @example
* Validate a numeric length before allocating buffers.
*
* ```ts
* anumber(1);
* ```
*/
const anumber$1 = anumber$2;
/**
* Asserts something is a plain object-ish value, not null or array.
* @param value - Value to validate.
* @param title - Label included in thrown errors.
* @returns The validated object.
* @throws On wrong argument types. {@link TypeError}
* @example
* Validate an options object before checking fields.
*
* ```ts
* aobject({ flag: true });
* ```
*/
function aobject$1(value, title = "object") {
	if (value === null || typeof value !== "object" || Array.isArray(value)) throw new TypeError(title === "object" ? "expected valid options object" : `"${title}" expected object, got type=${typeof value}`);
	return value;
}
/**
* Asserts something is a function.
* @param value - Value to validate.
* @param title - Label included in thrown errors.
* @returns The validated function.
* @throws On wrong argument types. {@link TypeError}
* @example
* Validate a required method before calling it.
*
* ```ts
* afunction(() => true, 'predicate');
* ```
*/
function afunction(value, title) {
	if (typeof value !== "function") throw new TypeError(`"${title}" is invalid: expected function, got ${typeof value}`);
	return value;
}
/**
* Encodes bytes as lowercase hex.
* @param bytes - Bytes to encode.
* @returns Lowercase hex string.
* @example
* Serialize bytes as hex for logging or fixtures.
*
* ```ts
* bytesToHex(Uint8Array.of(1, 2, 3));
* ```
*/
const bytesToHex = bytesToHex$1;
/**
* Decodes lowercase or uppercase hex into bytes.
* @param hex - Hex string to decode.
* @returns Decoded bytes.
* @example
* Parse fixture hex into bytes before hashing.
*
* ```ts
* hexToBytes('0102');
* ```
*/
const hexToBytes = (hex) => hexToBytes$1(hex);
/**
* Checks whether a value is a Uint8Array.
* @param a - Value to inspect.
* @returns `true` when `a` is a Uint8Array.
* @example
* Branch on byte input before decoding it.
*
* ```ts
* isBytes(new Uint8Array(1));
* ```
*/
const isBytes$1 = isBytes$2;
/**
* Reads random bytes from the platform CSPRNG.
* @param bytesLength - Number of random bytes to read.
* @returns Fresh random bytes.
* @example
* Generate a random seed for a keypair.
*
* ```ts
* randomBytes(2);
* ```
*/
const randomBytes = (bytesLength) => randomBytes$1(bytesLength);
const _0n$5 = /* @__PURE__ */ BigInt(0);
const _1n$5 = /* @__PURE__ */ BigInt(1);
const atitle$1 = (title) => title ? `"${title}" ` : "";
/**
* Validates that a flag is boolean.
* @param value - Value to validate.
* @param title - Optional field name.
* @returns Original value.
* @throws On wrong argument types. {@link TypeError}
* @example
* Reject non-boolean option flags early.
*
* ```ts
* abool(true);
* ```
*/
function abool$1(value, title = "") {
	if (typeof value !== "boolean") throw new TypeError(atitle$1(title) + "expected boolean, got type=" + typeof value);
	return value;
}
/**
* Validates that a value is a non-negative bigint or safe integer.
* @param n - Value to validate.
* @returns The same validated value.
* @throws On wrong argument ranges or values. {@link RangeError}
* @example
* Validate one integer-like value before serializing it.
*
* ```ts
* abignumber(1n);
* ```
*/
function abignumber(n) {
	if (typeof n === "bigint") {
		if (!isPosBig(n)) throw new RangeError("positive bigint expected, got " + n);
	} else anumber$1(n);
	return n;
}
/**
* Validates that a value is a safe integer.
* @param value - Integer to validate.
* @param title - Optional field name.
* @throws On wrong argument types. {@link TypeError}
* @throws On wrong argument ranges or values. {@link RangeError}
* @example
* Validate a window size before scalar arithmetic uses it.
*
* ```ts
* asafenumber(1);
* ```
*/
function asafenumber(value, title = "") {
	if (typeof value !== "number") {
		const prefix = title && `"${title}" `;
		throw new TypeError(prefix + "expected number, got type=" + typeof value);
	}
	if (!Number.isSafeInteger(value)) {
		const prefix = title && `"${title}" `;
		throw new RangeError(prefix + "expected safe integer, got " + value);
	}
}
/**
* Parses a big-endian hex string into bigint.
* Accepts odd-length hex through the native `BigInt('0x' + hex)` parser and currently surfaces the
* same native `SyntaxError` for malformed hex instead of wrapping it in a library-specific error.
* @param hex - Hex string without `0x`.
* @returns Parsed bigint value.
* @throws On wrong argument types. {@link TypeError}
* @example
* Parse a scalar from fixture hex.
*
* ```ts
* hexToNumber('ff');
* ```
*/
function hexToNumber(hex) {
	if (typeof hex !== "string") throw new TypeError("hex string expected, got " + typeof hex);
	return hex === "" ? _0n$5 : BigInt("0x" + hex);
}
/**
* Parses big-endian bytes into bigint.
* @param bytes - Bytes in big-endian order.
* @returns Parsed bigint value.
* @throws On wrong argument types. {@link TypeError}
* @example
* Read a scalar encoded in network byte order.
*
* ```ts
* bytesToNumberBE(Uint8Array.of(1, 0));
* ```
*/
function bytesToNumberBE(bytes) {
	return hexToNumber(bytesToHex$1(bytes));
}
/**
* Parses little-endian bytes into bigint.
* @param bytes - Bytes in little-endian order.
* @returns Parsed bigint value.
* @throws On wrong argument types. {@link TypeError}
* @example
* Read a scalar encoded in little-endian form.
*
* ```ts
* bytesToNumberLE(Uint8Array.of(1, 0));
* ```
*/
function bytesToNumberLE(bytes) {
	return hexToNumber(bytesToHex$1(copyBytes$1(abytes$2(bytes)).reverse()));
}
/**
* Encodes a bigint into fixed-length big-endian bytes.
* @param n - Number to encode.
* @param len - Output length in bytes. Must be greater than zero.
* @returns Big-endian byte array.
* @throws On wrong argument ranges or values. {@link RangeError}
* @throws If a documented runtime validation or state check fails. {@link Error}
* @example
* Serialize a scalar into a 32-byte field element.
*
* ```ts
* numberToBytesBE(255n, 2);
* ```
*/
function numberToBytesBE(n, len) {
	anumber$2(len);
	if (len === 0) throw new Error("zero output length is invalid");
	n = abignumber(n);
	const expectedLen = len * 2;
	const hex = n.toString(16);
	if (hex.length > expectedLen) throw new RangeError("number is too large");
	return hexToBytes$1(hex.padStart(expectedLen, "0"));
}
/**
* Encodes a bigint into fixed-length little-endian bytes.
* @param n - Number to encode.
* @param len - Output length in bytes.
* @returns Little-endian byte array.
* @throws On wrong argument ranges or values. {@link RangeError}
* @throws If a documented runtime validation or state check fails. {@link Error}
* @example
* Serialize a scalar for little-endian protocols.
*
* ```ts
* numberToBytesLE(255n, 2);
* ```
*/
function numberToBytesLE(n, len) {
	return numberToBytesBE(n, len).reverse();
}
/**
* Copies Uint8Array. We can't use u8a.slice(), because u8a can be Buffer,
* and Buffer#slice creates mutable copy. Never use Buffers!
* @param bytes - Bytes to copy.
* @returns Detached copy.
* @example
* Make an isolated copy before mutating serialized bytes.
*
* ```ts
* copyBytes(Uint8Array.of(1, 2, 3));
* ```
*/
function copyBytes$1(bytes) {
	return Uint8Array.from(abytes$1(bytes));
}
/**
* Checks whether n is non-negative bigint. Historical name.
* @param n - candidate value
* @returns `true` when the value is bigint and 0 or larger
* @example
* Check a candidate scalar before range validation.
*
* ```ts
* isPosBig(2n);
* ```
*/
function isPosBig(n) {
	return typeof n === "bigint" && _0n$5 <= n;
}
/**
* Checks whether a bigint lies inside a half-open range.
* @param n - Candidate value.
* @param min - Inclusive lower bound.
* @param max - Exclusive upper bound.
* @returns `true` when the value is inside the range.
* @example
* Check whether a candidate scalar fits the field order.
*
* ```ts
* inRange(2n, 1n, 3n);
* ```
*/
function inRange(n, min, max) {
	return isPosBig(n) && isPosBig(min) && isPosBig(max) && min <= n && n < max;
}
/**
* Asserts `min <= n < max`. NOTE: upper bound is exclusive.
* @param title - Value label for error messages.
* @param n - Candidate value.
* @param min - Inclusive lower bound.
* @param max - Exclusive upper bound.
* Wrong-type inputs are not separated from out-of-range values here: they still flow through the
* shared `RangeError` path because this is only a throwing wrapper around `inRange(...)`.
* @throws On wrong argument ranges or values. {@link RangeError}
* @example
* Assert that a bigint stays within one half-open range.
*
* ```ts
* aInRange('x', 2n, 1n, 256n);
* ```
*/
function aInRange(title, n, min, max) {
	if (!inRange(n, min, max)) throw new RangeError("expected valid " + title + ": " + min + " <= n < " + max + ", got " + n);
}
/**
* Calculates amount of bits in a bigint.
* Same as `n.toString(2).length`
* TODO: merge with nLength in modular
* @param n - Value to inspect.
* @returns Bit length.
* @throws If the value is negative. {@link Error}
* @example
* Measure the bit length of a scalar before serialization.
*
* ```ts
* bitLen(8n);
* ```
*/
function bitLen(n) {
	if (n < _0n$5) throw new Error("expected non-negative bigint, got " + n);
	return n === _0n$5 ? 0 : n.toString(2).length;
}
/**
* Calculate mask for N bits. Not using ** operator with bigints because of old engines.
* Same as BigInt(`0b${Array(i).fill('1').join('')}`)
* @param n - Number of bits. Negative widths are currently passed through to raw bigint shift
*   semantics and therefore produce `-1n`.
* @returns Bitmask value.
* @example
* Calculate mask for N bits.
*
* ```ts
* bitMask(4);
* ```
*/
const bitMask = (n) => {
	asafenumber(n, "n");
	return (_1n$5 << BigInt(n)) - _1n$5;
};
/**
* Validates declared required and optional field types on a plain object.
* Extra keys are intentionally ignored because many callers validate only the subset they use from
* richer option bags or runtime objects.
* This walks field schemas and formats detailed errors, so avoid it on hot paths; use direct
* one-line guards such as `aobject()`, `afunction()`, `abool()`, or `asafenumber()` instead.
* @param object - Object to validate.
* @param fields - Required field types.
* @param optFields - Optional field types.
* @param title - Object label included in thrown errors.
* @throws On wrong argument types. {@link TypeError}
* @example
* Check user options before building a curve helper.
*
* ```ts
* validateObject({ flag: true }, { flag: 'boolean' });
* ```
*/
function validateObject(object, fields = {}, optFields = {}, title = "object") {
	aobject$1(object, title);
	aobject$1(fields, "fields");
	aobject$1(optFields, "optFields");
	function checkField(fieldName, expectedType, isOpt) {
		const label = title === "object" ? `param "${String(fieldName)}"` : `"${title}.${String(fieldName)}"`;
		const val = object[fieldName];
		if (!Object.hasOwn(object, fieldName) && (isOpt ? val !== void 0 : expectedType !== "function")) throw new TypeError(`${label} is invalid: expected own property`);
		if (isOpt && val === void 0) return;
		const current = typeof val;
		if (current !== expectedType || val === null) throw new TypeError(`${label} is invalid: expected ${expectedType}, got ${current}`);
	}
	const iter = (f, isOpt) => Object.entries(f).forEach(([k, v]) => checkField(k, v, isOpt));
	iter(fields, false);
	iter(optFields, true);
}
//#endregion
//#region node_modules/@noble/curves/abstract/modular.js
/**
* Utils for modular division and fields.
* Field over 11 is a finite (Galois) field is integer number operations `mod 11`.
* There is no division: it is replaced by modular multiplicative inverse.
* @module
*/
/*! noble-curves - MIT License (c) 2022 Paul Miller (paulmillr.com) */
const _0n$4 = /* @__PURE__ */ BigInt(0);
const _1n$4 = /* @__PURE__ */ BigInt(1);
const _2n$3 = /* @__PURE__ */ BigInt(2);
const _3n$1 = /* @__PURE__ */ BigInt(3);
const _4n$2 = /* @__PURE__ */ BigInt(4);
const _5n$1 = /* @__PURE__ */ BigInt(5);
const _7n = /* @__PURE__ */ BigInt(7);
const _8n$2 = /* @__PURE__ */ BigInt(8);
const _9n = /* @__PURE__ */ BigInt(9);
const _15n = /* @__PURE__ */ BigInt(15);
const _16n = /* @__PURE__ */ BigInt(16);
const POW_WINDOWED_MIN = /* @__PURE__ */ BigInt("0x10000000000000000");
/**
* @param a - Dividend value.
* @param b - Positive modulus.
* @returns Reduced value in `[0, b)` only when `b` is positive.
* @throws If the modulus is not positive. {@link Error}
* @example
* Normalize a bigint into one field residue.
*
* ```ts
* mod(-1n, 5n);
* ```
*/
function mod(a, b) {
	if (b <= _0n$4) throw new Error("mod: expected positive modulus, got " + b);
	const result = a % b;
	return result >= _0n$4 ? result : b + result;
}
/**
* Efficiently raise num to a power with modular reduction.
* Unsafe in some contexts: uses ladder, so can expose bigint bits.
* Low-level helper: callers that need canonical residues must pass a valid `num` for the chosen
* modulus instead of relying on the `power===0/1` fast paths to normalize it.
* @param num - Base value.
* @param power - Exponent value.
* @param modulo - Reduction modulus.
* @returns Modular exponentiation result.
* @throws If the modulus or exponent is invalid. {@link Error}
* @example
* Raise one bigint to a modular power.
*
* ```ts
* pow(2n, 6n, 11n) // 64n % 11n == 9n
* ```
*/
function pow(num, power, modulo) {
	if (modulo <= _1n$4) throw new Error("pow: expected modulus > 1, got " + modulo);
	if (typeof power !== "bigint") throw new TypeError("invalid exponent: expected bigint, got " + typeof power);
	if (power < _0n$4) throw new Error("invalid exponent, negatives unsupported");
	if (power === _0n$4) return _1n$4;
	if (power === _1n$4) return num;
	let d = num % modulo;
	if (d < _0n$4) d += modulo;
	if (power < POW_WINDOWED_MIN) {
		let p = _1n$4;
		while (power > _0n$4) {
			if (power & _1n$4) p = p * d % modulo;
			d = d * d % modulo;
			power >>= _1n$4;
		}
		return p;
	}
	const digits = [];
	while (power > _0n$4) {
		digits.push(Number(power & _15n));
		power >>= _4n$2;
	}
	const table = new Array(16);
	table[0] = _1n$4;
	table[1] = d;
	for (let i = 2; i < 16; i++) table[i] = table[i - 1] * d % modulo;
	let p = table[digits[digits.length - 1]];
	for (let w = digits.length - 2; w >= 0; w--) {
		p = p * p % modulo;
		p = p * p % modulo;
		p = p * p % modulo;
		p = p * p % modulo;
		const digit = digits[w];
		if (digit !== 0) p = p * table[digit] % modulo;
	}
	return p;
}
/**
* Does `x^(2^power)` mod p. `pow2(30, 4)` == `30^(2^4)`.
* Low-level helper: callers that need canonical residues must pass a valid `x` for the chosen
* modulus; the `power===0` fast path intentionally returns the input unchanged.
* @param x - Base value.
* @param power - Number of squarings.
* @param modulo - Reduction modulus.
* @returns Repeated-squaring result.
* @throws If the exponent is negative. {@link Error}
* @example
* Apply repeated squaring inside one field.
*
* ```ts
* pow2(3n, 2n, 11n);
* ```
*/
function pow2(x, power, modulo) {
	if (modulo <= _1n$4) throw new Error("pow2: expected modulus > 1, got " + modulo);
	if (power < _0n$4) throw new Error("pow2: expected non-negative exponent, got " + power);
	let res = x;
	while (power-- > _0n$4) {
		res *= res;
		res %= modulo;
	}
	return res;
}
/**
* Inverses number over modulo.
* Implemented using the {@link https://brilliant.org/wiki/extended-euclidean-algorithm/ | extended Euclidean algorithm}.
* @param number - Value to invert.
* @param modulo - Modulus greater than 1.
* @returns Multiplicative inverse.
* @throws If the modulus is invalid or the inverse does not exist. {@link Error}
* @example
* Compute one modular inverse with the extended Euclidean algorithm.
*
* ```ts
* invert(3n, 11n);
* ```
*/
function invert(number, modulo) {
	if (number === _0n$4) throw new Error("invert: expected non-zero number");
	if (modulo <= _1n$4) throw new Error("invert: expected modulus > 1, got " + modulo);
	let a = mod(number, modulo);
	let b = modulo;
	let x = _0n$4, u = _1n$4;
	while (a !== _0n$4) {
		const q = b / a;
		const r = b - a * q;
		const m = x - u * q;
		b = a, a = r, x = u, u = m;
	}
	if (b !== _1n$4) throw new Error("invert: does not exist");
	return mod(x, modulo);
}
function assertIsSquare(Fp, root, n) {
	const F = Fp;
	if (!F.eql(F.sqr(root), n)) throw new Error("Cannot find square root");
}
function aoddModulus(order, fnName) {
	if ((order & _1n$4) === _0n$4) throw new Error(fnName + ": expected odd modulus, got " + order);
}
function sqrt3mod4(Fp, n) {
	const F = Fp;
	const p1div4 = (F.ORDER + _1n$4) / _4n$2;
	const root = F.pow(n, p1div4);
	assertIsSquare(F, root, n);
	return root;
}
function sqrt5mod8(Fp, n) {
	const F = Fp;
	const p5div8 = (F.ORDER - _5n$1) / _8n$2;
	const n2 = F.mul(n, _2n$3);
	const v = F.pow(n2, p5div8);
	const nv = F.mul(n, v);
	const i = F.mul(F.mul(nv, _2n$3), v);
	const root = F.mul(nv, F.sub(i, F.ONE));
	assertIsSquare(F, root, n);
	return root;
}
function sqrt9mod16(P) {
	const Fp_ = Field(P);
	const tn = tonelliShanks(P);
	const c1 = tn(Fp_, Fp_.neg(Fp_.ONE));
	const c2 = tn(Fp_, c1);
	const c3 = tn(Fp_, Fp_.neg(c1));
	const c4 = (P + _7n) / _16n;
	return ((Fp, n) => {
		const F = Fp;
		let tv1 = F.pow(n, c4);
		let tv2 = F.mul(tv1, c1);
		const tv3 = F.mul(tv1, c2);
		const tv4 = F.mul(tv1, c3);
		const e1 = F.eql(F.sqr(tv2), n);
		const e2 = F.eql(F.sqr(tv3), n);
		tv1 = F.cmov(tv1, tv2, e1);
		tv2 = F.cmov(tv4, tv3, e2);
		const e3 = F.eql(F.sqr(tv2), n);
		const root = F.cmov(tv1, tv2, e3);
		assertIsSquare(F, root, n);
		return root;
	});
}
/**
* Tonelli-Shanks square root search algorithm.
* This implementation is variable-time: it searches data-dependently for the first non-residue `Z`
* and for the smallest `i` in the main loop, unlike RFC 9380 Appendix I.4's constant-time shape.
* 1. {@link https://eprint.iacr.org/2012/685.pdf | eprint 2012/685}, page 12
* 2. Square Roots from 1; 24, 51, 10 to Dan Shanks
* @param P - field order
* @returns function that takes field Fp (created from P) and number n
* @throws If the field is too small, non-prime, or the square root does not exist. {@link Error}
* @example
* Construct a square-root helper for primes that need Tonelli-Shanks.
*
* ```ts
* import { Field, tonelliShanks } from '@noble/curves/abstract/modular.js';
* const Fp = Field(17n);
* const sqrt = tonelliShanks(17n)(Fp, 4n);
* ```
*/
function tonelliShanks(P) {
	if (P < _3n$1) throw new Error("sqrt is not defined for small field");
	aoddModulus(P, "tonelliShanks");
	let Q = P - _1n$4;
	let S = 0;
	while (Q % _2n$3 === _0n$4) {
		Q /= _2n$3;
		S++;
	}
	let Z = _2n$3;
	const _Fp = Field(P);
	while (FpLegendre(_Fp, Z) === 1) if (Z++ > 1e3) throw new Error("Cannot find square root: probably non-prime P");
	if (S === 1) return sqrt3mod4;
	let cc = _Fp.pow(Z, Q);
	const Q1div2 = (Q + _1n$4) / _2n$3;
	return function tonelliSlow(Fp, n) {
		const F = Fp;
		if (F.is0(n)) return n;
		if (FpLegendre(F, n) !== 1) throw new Error("Cannot find square root");
		let M = S;
		let c = F.mul(F.ONE, cc);
		let t = F.pow(n, Q);
		let R = F.pow(n, Q1div2);
		while (!F.eql(t, F.ONE)) {
			if (F.is0(t)) throw new Error("Cannot find square root: probably non-prime P");
			let i = 1;
			let t_tmp = F.sqr(t);
			while (!F.eql(t_tmp, F.ONE)) {
				i++;
				t_tmp = F.sqr(t_tmp);
				if (i === M) throw new Error("Cannot find square root");
			}
			const exponent = _1n$4 << BigInt(M - i - 1);
			const b = F.pow(c, exponent);
			M = i;
			c = F.sqr(b);
			t = F.mul(t, c);
			R = F.mul(R, b);
		}
		return R;
	};
}
/**
* Square root for a finite field. Will try optimized versions first:
*
* 1. P ≡ 3 (mod 4)
* 2. P ≡ 5 (mod 8)
* 3. P ≡ 9 (mod 16)
* 4. Tonelli-Shanks algorithm
*
* Different algorithms can give different roots, it is up to user to decide which one they want.
* For example there is FpSqrtOdd/FpSqrtEven to choose a root by oddness
* (used for hash-to-curve).
* @param P - Field order.
* @returns Square-root helper. The generic fallback inherits Tonelli-Shanks' variable-time
*   behavior and this selector assumes prime-field-style integer moduli.
* @throws If the field is unsupported or the square root does not exist. {@link Error}
* @example
* Choose the square-root helper appropriate for one field modulus.
*
* ```ts
* import { Field, FpSqrt } from '@noble/curves/abstract/modular.js';
* const Fp = Field(17n);
* const sqrt = FpSqrt(17n)(Fp, 4n);
* ```
*/
function FpSqrt(P) {
	aoddModulus(P, "Fp.sqrt");
	if (P % _4n$2 === _3n$1) return sqrt3mod4;
	if (P % _8n$2 === _5n$1) return sqrt5mod8;
	if (P % _16n === _9n) return sqrt9mod16(P);
	return tonelliShanks(P);
}
/**
* @param num - Value to inspect.
* @param modulo - Field modulus.
* @returns `true` when the least-significant little-endian bit is set.
* @throws If the modulus is invalid for `mod(...)`. {@link Error}
* @example
* Inspect the low bit used by little-endian sign conventions.
*
* ```ts
* isNegativeLE(3n, 11n);
* ```
*/
const isNegativeLE = (num, modulo) => (mod(num, modulo) & _1n$4) === _1n$4;
const FIELD_FIELDS = [
	"create",
	"isValid",
	"is0",
	"neg",
	"inv",
	"sqrt",
	"sqr",
	"eql",
	"add",
	"sub",
	"mul",
	"pow",
	"div",
	"addN",
	"subN",
	"mulN",
	"sqrN"
];
/**
* @param field - Field implementation.
* @returns Validated field. This only checks the arithmetic subset needed by generic helpers; it
*   does not guarantee full runtime-method coverage for serialization, batching, `cmov`, or
*   field-specific extras beyond positive `BYTES` / `BITS`.
* @throws If the field shape or numeric metadata are invalid. {@link Error}
* @example
* Check that a field implementation exposes the operations curve code expects.
*
* ```ts
* import { Field, validateField } from '@noble/curves/abstract/modular.js';
* const Fp = validateField(Field(17n));
* ```
*/
function validateField(field) {
	aobject$1(field, "field");
	if (typeof field.ORDER !== "bigint") throw new TypeError("param \"ORDER\" is invalid: expected bigint, got " + typeof field.ORDER);
	asafenumber(field.BYTES, "BYTES");
	asafenumber(field.BITS, "BITS");
	for (const name of FIELD_FIELDS) afunction(field[name], "field." + name);
	if (field.BYTES < 1 || field.BITS < 1) throw new Error("invalid field: expected BYTES/BITS > 0");
	if (field.ORDER <= _1n$4) throw new Error("invalid field: expected ORDER > 1, got " + field.ORDER);
	return field;
}
function FpInvertBatch(Fp, nums, passZero = false) {
	validateField(Fp);
	aarray(nums, "nums");
	abool$1(passZero, "passZero");
	const F = Fp;
	const inverted = new Array(nums.length).fill(passZero ? F.ZERO : void 0);
	const multipliedAcc = nums.reduce((acc, num, i) => {
		if (F.is0(num)) return acc;
		inverted[i] = acc;
		return F.mul(acc, num);
	}, F.ONE);
	const invertedAcc = F.inv(multipliedAcc);
	nums.reduceRight((acc, num, i) => {
		if (F.is0(num)) return acc;
		inverted[i] = F.mul(acc, inverted[i]);
		return F.mul(acc, num);
	}, invertedAcc);
	return inverted;
}
/**
* Legendre symbol.
* Legendre constant is used to calculate Legendre symbol (a | p)
* which denotes the value of a^((p-1)/2) (mod p).
*
* * (a | p) ≡ 1    if a is a square (mod p), quadratic residue
* * (a | p) ≡ -1   if a is not a square (mod p), quadratic non residue
* * (a | p) ≡ 0    if a ≡ 0 (mod p)
* @param Fp - Field implementation.
* @param n - Value to inspect.
* @returns Legendre symbol.
* @throws If the powered value does not match a valid Legendre symbol. {@link Error}
* @example
* Compute the Legendre symbol of one field element.
*
* ```ts
* import { Field, FpLegendre } from '@noble/curves/abstract/modular.js';
* const Fp = Field(17n);
* const symbol = FpLegendre(Fp, 4n);
* ```
*/
function FpLegendre(Fp, n) {
	validateField(Fp);
	const F = Fp;
	aoddModulus(F.ORDER, "FpLegendre");
	const p1mod2 = (F.ORDER - _1n$4) / _2n$3;
	const powered = F.pow(n, p1mod2);
	const yes = F.eql(powered, F.ONE);
	const zero = F.eql(powered, F.ZERO);
	const no = F.eql(powered, F.neg(F.ONE));
	if (!yes && !zero && !no) throw new Error("invalid Legendre symbol result");
	return yes ? 1 : zero ? 0 : -1;
}
/**
* @param n - Curve order. Callers are expected to pass a positive order.
* @param nBitLength - Optional cached bit length. Callers are expected to pass a positive cached
*   value when overriding the derived bit length.
* @returns Byte and bit lengths.
* @throws If the order or cached bit length is invalid. {@link Error}
* @example
* Measure the encoding sizes needed for one modulus.
*
* ```ts
* nLength(255n);
* ```
*/
function nLength(n, nBitLength) {
	if (nBitLength !== void 0) anumber$1(nBitLength);
	if (n <= _0n$4) throw new Error("invalid n length: expected positive n, got " + n);
	if (nBitLength !== void 0 && nBitLength < 1) throw new Error("invalid n length: expected positive bit length, got " + nBitLength);
	const bits = bitLen(n);
	if (nBitLength !== void 0 && nBitLength < bits) throw new Error(`invalid n length: expected nBitLength (${nBitLength}) >= bitLen(n) (${bits})`);
	const _nBitLength = nBitLength !== void 0 ? nBitLength : bits;
	return {
		nBitLength: _nBitLength,
		nByteLength: Math.ceil(_nBitLength / 8)
	};
}
const FIELD_SQRT = /* @__PURE__ */ new WeakMap();
var _Field = class {
	ORDER;
	BITS;
	BYTES;
	isLE;
	ZERO = _0n$4;
	ONE = _1n$4;
	_lengths;
	_mod;
	constructor(ORDER, opts = {}) {
		if (ORDER <= _1n$4) throw new Error("invalid field: expected ORDER > 1, got " + ORDER);
		let _nbitLength = void 0;
		this.isLE = false;
		if (opts != null && typeof opts === "object") {
			if (typeof opts.BITS === "number") _nbitLength = opts.BITS;
			if (typeof opts.sqrt === "function") Object.defineProperty(this, "sqrt", {
				value: opts.sqrt,
				enumerable: true
			});
			if (typeof opts.isLE === "boolean") this.isLE = opts.isLE;
			if (opts.allowedLengths) this._lengths = Object.freeze(opts.allowedLengths.slice());
			if (typeof opts.modFromBytes === "boolean") this._mod = opts.modFromBytes;
		}
		const { nBitLength, nByteLength } = nLength(ORDER, _nbitLength);
		if (nByteLength > 2048) throw new Error("invalid field: expected ORDER of <= 2048 bytes");
		this.ORDER = ORDER;
		this.BITS = nBitLength;
		this.BYTES = nByteLength;
		Object.freeze(this);
	}
	create(num) {
		return mod(num, this.ORDER);
	}
	isValid(num) {
		if (typeof num !== "bigint") throw new TypeError("invalid field element: expected bigint, got " + typeof num);
		return _0n$4 <= num && num < this.ORDER;
	}
	is0(num) {
		return num === _0n$4;
	}
	isValidNot0(num) {
		return !this.is0(num) && this.isValid(num);
	}
	isOdd(num) {
		return (num & _1n$4) === _1n$4;
	}
	neg(num) {
		return mod(-num, this.ORDER);
	}
	eql(lhs, rhs) {
		return lhs === rhs;
	}
	sqr(num) {
		return mod(num * num, this.ORDER);
	}
	add(lhs, rhs) {
		return mod(lhs + rhs, this.ORDER);
	}
	sub(lhs, rhs) {
		return mod(lhs - rhs, this.ORDER);
	}
	mul(lhs, rhs) {
		return mod(lhs * rhs, this.ORDER);
	}
	pow(num, power) {
		return pow(num, power, this.ORDER);
	}
	div(lhs, rhs) {
		return mod(lhs * invert(rhs, this.ORDER), this.ORDER);
	}
	sqrN(num) {
		return num * num;
	}
	addN(lhs, rhs) {
		return lhs + rhs;
	}
	subN(lhs, rhs) {
		return lhs - rhs;
	}
	mulN(lhs, rhs) {
		return lhs * rhs;
	}
	inv(num) {
		return invert(num, this.ORDER);
	}
	sqrt(num) {
		let sqrt = FIELD_SQRT.get(this);
		if (!sqrt) FIELD_SQRT.set(this, sqrt = FpSqrt(this.ORDER));
		return sqrt(this, num);
	}
	toBytes(num) {
		return this.isLE ? numberToBytesLE(num, this.BYTES) : numberToBytesBE(num, this.BYTES);
	}
	fromBytes(bytes, skipValidation = false) {
		abytes$1(bytes);
		const { _lengths: allowedLengths, BYTES, isLE, ORDER, _mod: modFromBytes } = this;
		if (allowedLengths) {
			if (bytes.length < 1 || !allowedLengths.includes(bytes.length) || bytes.length > BYTES) throw new Error("Field.fromBytes: expected " + allowedLengths + " bytes, got " + bytes.length);
			const padded = new Uint8Array(BYTES);
			padded.set(bytes, isLE ? 0 : padded.length - bytes.length);
			bytes = padded;
		}
		if (bytes.length !== BYTES) throw new Error("Field.fromBytes: expected " + BYTES + " bytes, got " + bytes.length);
		let scalar = isLE ? bytesToNumberLE(bytes) : bytesToNumberBE(bytes);
		if (modFromBytes) scalar = mod(scalar, ORDER);
		if (!skipValidation) {
			if (!this.isValid(scalar)) throw new Error("invalid field element: outside of range 0..ORDER");
		}
		return scalar;
	}
	invertBatch(lst) {
		return FpInvertBatch(this, lst, true);
	}
	cmov(a, b, condition) {
		abool$1(condition, "condition");
		return condition ? b : a;
	}
};
/**
* Creates a finite field. Major performance optimizations:
* * 1. Denormalized operations like mulN instead of mul.
* * 2. Identical object shape: never add or remove keys.
* * 3. Frozen stable object shape; the lazy sqrt cache lives in a module-level `WeakMap`.
* Fragile: always run a benchmark on a change.
* Security note: operations and low-level serializers like `toBytes` don't check `isValid` for
* all elements for performance and protocol-flexibility reasons; callers are responsible for
* supplying valid elements when they need canonical field behavior.
* This is low-level code, please make sure you know what you're doing.
*
* Note about field properties:
* * CHARACTERISTIC p = prime number, number of elements in main subgroup.
* * ORDER q = similar to cofactor in curves, may be composite `q = p^m`.
*
* @param ORDER - field order, probably prime, or could be composite
* @param opts - Field options such as bit length or endianness. See {@link FieldOpts}.
* @returns Frozen field instance with a stable object shape. This wrapper forwards `opts` straight
*   into `_Field`, so it inherits `_Field`'s assumptions about cached sizes and `allowedLengths`.
* @example
* Construct one prime field with optional overrides.
*
* ```ts
* Field(11n);
* ```
*/
function Field(ORDER, opts = {}) {
	Object.freeze(_Field.prototype);
	return new _Field(ORDER, opts);
}
//#endregion
//#region node_modules/@noble/curves/abstract/curve.js
/**
* Methods for elliptic curve multiplication by scalars.
* Contains wNAF-based ScalarMultiplier, pippenger.
* @module
*/
/*! noble-curves - MIT License (c) 2022 Paul Miller (paulmillr.com) */
const _0n$3 = /* @__PURE__ */ BigInt(0);
const _1n$3 = /* @__PURE__ */ BigInt(1);
const _4n$1 = /* @__PURE__ */ BigInt(4);
const BLIND_BYTES = 16;
const BLIND_BITS = 128;
const FW_WINDOW = 5;
const TABLE_BYTES_MAX = /* @__PURE__ */ (() => 2 ** 31)();
/**
* Validates the static surface of a point constructor.
* This is only a cheap sanity check for the constructor hooks and fields consumed by generic
* factories; it does not certify `BASE`/`ZERO` semantics or prove the curve implementation itself.
* @param Point - Runtime point constructor.
* @throws On missing constructor hooks or malformed field metadata. {@link TypeError}
* @example
* Check that one point constructor exposes the static hooks generic helpers need.
*
* ```ts
* import { ed25519 } from '@noble/curves/ed25519.js';
* import { validatePointCons } from '@noble/curves/abstract/curve.js';
* validatePointCons(ed25519.Point);
* ```
*/
function validatePointCons(Point) {
	const pc = Point;
	if (typeof pc !== "function") throw new TypeError("\"Point\" expected constructor, got type=" + typeof Point);
	afunction(pc.fromAffine, "Point.fromAffine");
	afunction(pc.fromBytes, "Point.fromBytes");
	afunction(pc.fromHex, "Point.fromHex");
	aobject$1(pc.BASE, "Point.BASE");
	aobject$1(pc.ZERO, "Point.ZERO");
	validateField(pc.Fp);
	validateField(pc.Fn);
}
/**
* Takes a bunch of Projective Points but executes only one
* inversion on all of them. Inversion is very slow operation,
* so this improves performance massively.
* Optimization: converts a list of projective points to a list of identical points with Z=1.
* Input points are left unchanged; the normalized points are returned as fresh instances.
* @param c - Point constructor.
* @param points - Projective points.
* @returns Fresh projective points reconstructed from normalized affine coordinates.
* @example
* Batch-normalize projective points with a single shared inversion.
*
* ```ts
* import { normalizeZ } from '@noble/curves/abstract/curve.js';
* import { p256 } from '@noble/curves/nist.js';
* const points = normalizeZ(p256.Point, [p256.Point.BASE, p256.Point.BASE.double()]);
* ```
*/
function normalizeZ(c, points) {
	validatePointCons(c);
	validateMSMPoints(points, c);
	const invertedZs = FpInvertBatch(c.Fp, points.map((p) => p.Z));
	return points.map((p, i) => c.fromAffine(p.toAffine(invertedZs[i])));
}
function validateW(W, bits, min = 1) {
	if (!Number.isSafeInteger(W) || W < min || W > bits) throw new Error("invalid window size, expected [" + min + ".." + bits + "], got W=" + W);
}
function validateTableBytes(numPoints, fpBytes) {
	const bytes = numPoints * (4 * fpBytes + 128);
	if (bytes > TABLE_BYTES_MAX) throw new Error("invalid window size: table would need ~" + Math.ceil(bytes / 2 ** 20) + " MiB, max " + TABLE_BYTES_MAX / 2 ** 20 + " MiB");
}
/**
* Probes an RNG once, at construction time: returns `undefined` when it is unavailable —
* throws or returns malformed bytes — so callers can downgrade to their unblinded /
* deterministic constant-time fallback. Blinding is defense-in-depth (DPA/template
* hardening), not a correctness or key-secrecy requirement, so availability-based
* downgrade is acceptable.
*
* The downgrade decision is deliberately static. After a successful probe the RNG becomes
* part of the trusted contract: later misbehavior must fail closed in per-call validation
* (throw), never downgrade — a dynamic fallback would let a tampered RNG silently strip
* blinding on demand. A probe can only ever classify broken environments, not adversarial
* RNGs: a stateful RNG can always behave while probed and misbehave later.
* @param randomBytes - RNG to probe, or `undefined` when the environment provides none.
* @param length - Byte length requested from the probe call.
* @returns The RNG when the probe produced `length` valid bytes; `undefined` otherwise.
* @example
* Probe an RNG once before enabling scalar blinding.
*
* ```ts
* import { probeRandomBytes } from '@noble/curves/abstract/curve.js';
* import { randomBytes } from '@noble/hashes/utils.js';
* const rng = probeRandomBytes(randomBytes, 16);
* ```
*/
function probeRandomBytes(randomBytes, length) {
	if (randomBytes === void 0) return void 0;
	afunction(randomBytes, "randomBytes");
	try {
		const probe = randomBytes(length);
		if (!isBytes$1(probe) || probe.length !== length) return void 0;
	} catch {
		return;
	}
	return randomBytes;
}
function validateMSMPoints(points, c) {
	aarray(points, "points");
	points.forEach((p, i) => {
		if (!(p instanceof c)) throw new Error("invalid point at index " + i);
	});
}
function validateMSMScalars(scalars, field, maxScalar) {
	if (!Array.isArray(scalars)) throw new Error("array of scalars expected");
	scalars.forEach((s, i) => {
		if (!(maxScalar === void 0 ? field.isValid(s) : isPosBig(s) && s < maxScalar)) throw new Error("invalid scalar at index " + i);
	});
}
const pointWindowSizes = /* @__PURE__ */ new WeakMap();
function getWindowSize(P) {
	return pointWindowSizes.get(P) || 1;
}
/** Table of odd multiples [1P, 3P, ..., (2⋅size−1)P]; width-W wNAF uses size = 2^(W−2). */
function oddMultiples(p, size) {
	const dbl = p.double();
	const t = [p];
	for (let j = 1; j < size; j++) t.push(t[j - 1].add(dbl));
	return t;
}
/**
* Width-W wNAF signed-digit recoding (W >= 2), LSB-first: digits are 0 or odd with
* |digit| < 2^(W−1); nonzero density ~1/(W+1) (a nonzero digit is followed by W−1 zeros).
*/
function wnafDigits(n, W) {
	const size = 2 ** W;
	const half = size / 2;
	const mask = BigInt(size - 1);
	const d = [];
	while (n > _0n$3) {
		let w = 0;
		if (n & _1n$3) {
			w = Number(n & mask);
			if (w >= half) w -= size;
			n -= BigInt(w);
		}
		d.push(w);
		n >>= _1n$3;
	}
	return d;
}
/**
* Fixed-position signed-window recoding for precomputed wNAF: `n = Σ digits[w]⋅2^(w⋅W)` with
* digits in `[−2^(W−1)+1, 2^(W−1)]`. Digit count is fixed by `windows` (callers reserve one
* extra window for the final carry), so recoding length does not depend on the scalar.
*/
function signedWindowDigits(n, W, windows) {
	const size = 2 ** W;
	const half = size / 2;
	const mask = BigInt(size - 1);
	const shiftBy = BigInt(W);
	const d = [];
	for (let w = 0; w < windows; w++) {
		let v = Number(n & mask);
		n >>= shiftBy;
		if (v > half) {
			v -= size;
			n += _1n$3;
		}
		d.push(v);
	}
	if (n !== _0n$3) throw new Error("invalid wnaf");
	return d;
}
/**
* Shared vartime walk over per-scalar wNAF digit streams: one doubling of a single shared
* accumulator per bit position of the longest recoding, one signed table addition per
* nonzero digit. `tables[i]` must hold the odd multiples of the i-th point.
*/
function wnafWalk(zero, tables, digits) {
	let max = 0;
	for (const d of digits) max = Math.max(max, d.length);
	let acc = zero;
	for (let bit = max - 1; bit >= 0; bit--) {
		if (bit !== max - 1) acc = acc.double();
		for (let i = 0; i < digits.length; i++) {
			const w = digits[i][bit];
			if (w) {
				const item = tables[i][Math.abs(w) - 1 >> 1];
				acc = acc.add(w < 0 ? item.negate() : item);
			}
		}
	}
	return acc;
}
/**
* Elliptic curve multiplication of Point by scalar.
* Routes between cached-table, fixed-window, and one-shot wNAF paths; entry points validate
* their own scalars (`mulCT`/`mulCTBlinded`: `1 <= s < Fn.ORDER`; `mulUnsafe`: up to the
* `Fn.ORDER^4` DoS cap via {@link mulAddUnsafe}).
* Table generation is expensive and happens on first call of `multiply()`
* (or eagerly via `precompute(W, false)`). By default, `BASE` point is precomputed.
*
* Cached algorithm is signed fixed-window wNAF:
* - table stores, for every window w, the multiples `[1..2^(W−1)]⋅2^(w⋅W)⋅P` — all doublings
*   are baked in, so a multiplication is exactly one table addition per window
* - window count is fixed (`ceil(bits/W) + 1`), so the point-operation count is scalar-independent
*   (basis of the constant-time path)
* - for a 256-bit curve and W=6: 44⋅32 = 1408 table points, 44 additions per multiply
* - secret scalars are additionally blinded (see {@link ScalarMultiplier.mulCTBlinded}), which
*   widens tables by 128 bits
* @param Point - Point constructor.
* @param randomBytes - RNG used for scalar blinding; required by the blinded secret path.
* @example
* Elliptic curve multiplication of Point by scalar.
*
* ```ts
* import { ScalarMultiplier } from '@noble/curves/abstract/curve.js';
* import { p256 } from '@noble/curves/nist.js';
* const mul = new ScalarMultiplier(p256.Point);
* ```
*/
var ScalarMultiplier = class {
	Point;
	BASE;
	ZERO;
	randomBytes;
	wnafPrecomputes = /* @__PURE__ */ new WeakMap();
	baseCanBeBlinded;
	bits;
	constructor(Point, randomBytes) {
		validatePointCons(Point);
		this.randomBytes = probeRandomBytes(randomBytes, BLIND_BYTES);
		this.Point = Point;
		this.BASE = Point.BASE;
		this.ZERO = Point.ZERO;
		this.bits = Point.Fn.BITS;
	}
	/**
	* Creates a signed fixed-window wNAF precomputation table: for every window w, the
	* multiples `[1..2^(W−1)]⋅2^(w⋅W)⋅P`, flattened. All doublings are baked into the table,
	* so cached multiplication is additions-only. `windows = ceil(bits/W) + 1`: the extra
	* window absorbs the final carry of signed-digit recoding.
	* For a 256-bit curve and W=6, the table is 44⋅32 = 1408 points.
	* @param point - Point instance
	* @param W - window size
	* @param bits - scalar bitlength the table must cover
	*/
	buildWnafTable(point, W, bits) {
		const windows = Math.ceil(bits / W) + 1;
		const half = 2 ** (W - 1);
		const comp = [];
		let base = point;
		for (let w = 0; w < windows; w++) {
			let acc = base;
			for (let i = 0; i < half; i++) {
				comp.push(acc);
				acc = acc.add(base);
			}
			base = comp[comp.length - 1].double();
		}
		return {
			W,
			bits,
			windows,
			comp
		};
	}
	/**
	* Implements ec multiplication using precomputed signed fixed-window wNAF tables.
	* Constant-time: fixed window count with one table addition per window — zero digits feed
	* the fake accumulator — and no doublings; the lookup scans the whole window slice.
	* Scalar bounds are validated by the public entry points ({@link ScalarMultiplier.mulCT},
	* {@link ScalarMultiplier.mulCTBlinded}, {@link ScalarMultiplier.mulUnsafe});
	* signedWindowDigits throws if `n` exceeds the table.
	* @returns real and fake (for const-time) points
	*/
	wnafCachedCT(precomputes, n) {
		const { W, windows, comp } = precomputes;
		const half = 2 ** (W - 1);
		const digits = signedWindowDigits(n, W, windows);
		let p = this.ZERO;
		let f = this.BASE;
		for (let w = 0; w < windows; w++) {
			const digit = digits[w];
			const start = w * half;
			const idx = Math.abs(digit) - 1;
			let sel = comp[start];
			for (let i = 1; i < half; i++) sel = i === idx ? comp[start + i] : sel;
			const neg = sel.negate();
			if (digit === 0) f = f.add(comp[start]);
			else p = p.add(digit < 0 ? neg : sel);
		}
		return {
			p,
			f
		};
	}
	getWnafPrecomputes(W, point, bits, transform) {
		let entries = this.wnafPrecomputes.get(point);
		let comp = entries?.find((entry) => entry.W === W && entry.bits === bits);
		if (!comp) {
			comp = this.buildWnafTable(point, W, bits);
			if (typeof transform === "function") comp = {
				...comp,
				comp: transform(comp.comp)
			};
			if (!entries) {
				entries = [];
				this.wnafPrecomputes.set(point, entries);
			}
			entries.push(comp);
		}
		return comp;
	}
	assertPoint(point) {
		if (!(point instanceof this.Point)) throw new TypeError("\"point\" expected Point instance, got type=" + typeof point);
	}
	validateMulInput(point, scalar) {
		this.assertPoint(point);
		if (!inRange(scalar, _1n$3, this.Point.Fn.ORDER)) throw new Error("invalid scalar");
	}
	runCT(point, n, bits, transform) {
		const W = getWindowSize(point);
		if (W === 1) return this.fixedWindowCT(point, n, bits);
		return this.wnafCachedCT(this.getWnafPrecomputes(W, point, bits, transform), n);
	}
	mulCT(point, scalar, transform) {
		this.validateMulInput(point, scalar);
		return this.runCT(point, scalar, this.bits, transform);
	}
	mulCTBlinded(point, scalar, transform) {
		this.validateMulInput(point, scalar);
		if (this.randomBytes === void 0) throw new Error("randomBytes is required for scalar blinding");
		const bits = this.Point.Fn.BITS + BLIND_BITS;
		const blind = this.randomBytes(BLIND_BYTES);
		if (!isBytes$1(blind) || blind.length !== BLIND_BYTES) throw new Error("randomBytes returned invalid byte array");
		blind[0] = blind[0] & 63 | 128;
		const n = scalar + bytesToNumberBE(blind) * this.Point.Fn.ORDER;
		return this.runCT(point, n, bits, transform);
	}
	/**
	* Constant-time multiplication `n*point` for an un-precomputed point, via a small fixed window.
	* A cached wNAF table only pays off when reused; a flat 2^FW_WINDOW table (`size-1` adds) is
	* far cheaper to build for a single use. The point-operation sequence is independent of `n`:
	* build the table, then per window exactly FW_WINDOW doublings, a data-oblivious scan over
	* every table entry, and one addition (adds the identity when the window digit is 0 — never
	* skipped).
	*
	* `n` must be `< 2^bits`. Assumes complete addition (adding the identity costs the same as any
	* add), which holds for the Weierstrass/Edwards point types used here. The table is left in
	* projective form (no normalizeZ): normalizing this small a table costs more than the
	* mixed-add savings it would buy for a single multiply.
	* @returns real point `p`; `f` duplicates it only to match {@link wnafCachedCT}'s return shape
	* (this path needs no fake accumulator — its op-count is already scalar-independent).
	*/
	fixedWindowCT(point, n, bits) {
		const W = FW_WINDOW;
		const size = 32;
		const mask = bitMask(W);
		const table = new Array(size);
		table[0] = this.ZERO;
		for (let i = 1; i < size; i++) table[i] = table[i - 1].add(point);
		const windows = Math.ceil(bits / W);
		let acc = this.ZERO;
		for (let window = windows - 1; window >= 0; window--) {
			if (window !== windows - 1) for (let d = 0; d < W; d++) acc = acc.double();
			const digit = Number(n >> BigInt(window * W) & mask);
			let sel = table[0];
			for (let i = 1; i < size; i++) sel = i === digit ? table[i] : sel;
			acc = acc.add(sel);
		}
		return {
			p: acc,
			f: acc
		};
	}
	shouldBlind(point, cofactor) {
		if (this.randomBytes === void 0) return false;
		if (cofactor === _1n$3) return true;
		if (point !== this.BASE) return false;
		if (this.baseCanBeBlinded === void 0) this.baseCanBeBlinded = this.mulUnsafe(this.BASE, this.Point.Fn.ORDER).is0();
		return this.baseCanBeBlinded;
	}
	mulSecret(point, scalar, cofactor, transform) {
		return this.shouldBlind(point, cofactor) ? this.mulCTBlinded(point, scalar, transform) : this.mulCT(point, scalar, transform);
	}
	mulUnsafe(point, scalar, transform) {
		this.assertPoint(point);
		if (!isPosBig(scalar)) throw new Error("invalid scalar");
		const W = getWindowSize(point);
		if (W === 1 || scalar >= this.Point.Fn.ORDER) return mulAddUnsafe(this.Point, [point], [scalar], true);
		const precomputes = this.getWnafPrecomputes(W, point, this.bits, transform);
		return this.wnafCachedCT(precomputes, scalar).p;
	}
	setWindowSize(point, W) {
		this.assertPoint(point);
		validateW(W, this.bits);
		validateTableBytes((Math.ceil((this.bits + BLIND_BITS) / W) + 1) * 2 ** (W - 1), this.Point.Fp.BYTES);
		pointWindowSizes.set(point, W);
		this.wnafPrecomputes.delete(point);
	}
	hasWindowSize(point) {
		return getWindowSize(point) !== 1;
	}
};
/**
* Combined multi-scalar multiplication `Σ scalars[i]⋅points[i]` via interleaved width-4 wNAF
* (Strauss–Shamir). Every input gets its own table of odd multiples `[1P, 3P, 5P, 7P]` and
* signed-digit recoding, but all walks share one doubling chain, so total cost is
* `~bits` doublings + `L⋅bits/5` additions instead of `L⋅bits` doublings for separate
* multiplications. Intended for the 2-4 point shapes of signature verification
* (`R = u1⋅G + u2⋅P`); use {@link pippenger} for larger batches.
*
* Not constant-time: only for public inputs. Scalars must satisfy `0 <= s < Fn.ORDER`;
* fold negative signs into the points before calling.
* @param c - Point constructor.
* @param points - Array of curve points.
* @param scalars - Array of non-negative scalars, same length as points.
* @param allowOversized - Replace the `s < Fn.ORDER` scalar check with a `Fn.ORDER^4` DoS cap.
*   Off by default. For scalars that must NOT be reduced mod ORDER: torsion checks
*   (`Fn.ORDER⋅P ≟ O`) and cofactor-clearing multiples. Walk length grows with `bitLen(s)`.
* @returns Combined multiplication result; identity for empty input.
* @throws If the point set or scalar set is invalid. {@link Error}
* @example
* Combined multi-scalar multiplication via Strauss–Shamir.
*
* ```ts
* import { mulAddUnsafe } from '@noble/curves/abstract/curve.js';
* import { p256 } from '@noble/curves/nist.js';
* const G = p256.Point.BASE;
* const R = mulAddUnsafe(p256.Point, [G, G.double()], [2n, 3n]); // 2⋅G + 3⋅(2⋅G)
* ```
*/
function mulAddUnsafe(c, points, scalars, allowOversized = false) {
	validatePointCons(c);
	validateMSMPoints(points, c);
	abool$1(allowOversized, "allowOversized");
	validateMSMScalars(scalars, c.Fn, allowOversized ? c.Fn.ORDER ** _4n$1 : void 0);
	if (points.length !== scalars.length) throw new Error("arrays of points and scalars must have equal length");
	const tables = points.map((p) => oddMultiples(p, 4));
	const digits = scalars.map((n) => wnafDigits(n, 4));
	return wnafWalk(c.ZERO, tables, digits);
}
function createField(order, field, isLE) {
	if (field) {
		if (field.ORDER !== order) throw new Error("Field.ORDER must match order: Fp == p, Fn == n");
		validateField(field);
		return field;
	} else return Field(order, { isLE });
}
/**
* Validates basic CURVE shape and field membership, then creates fields.
* This does not prove that the generator is on-curve, that subgroup/order data are consistent, or
* that the curve equation itself is otherwise sane.
* @param type - Curve family.
* @param CURVE - Curve parameters.
* @param curveOpts - Optional field overrides. See {@link FpFn}:
*   - `Fp` (optional): Optional base-field override.
*   - `Fn` (optional): Optional scalar-field override.
* @param FpFnLE - Whether field encoding is little-endian.
* @returns Frozen curve parameters and fields.
* @throws If the curve parameters or field overrides are invalid. {@link Error}
* @example
* Build curve fields from raw constants before constructing a curve instance.
*
* ```ts
* const curve = createCurveFields('weierstrass', {
*   p: 17n,
*   n: 19n,
*   h: 1n,
*   a: 2n,
*   b: 2n,
*   Gx: 5n,
*   Gy: 1n,
* });
* ```
*/
function createCurveFields(type, CURVE, curveOpts = {}, FpFnLE) {
	if (type !== "weierstrass" && type !== "edwards") throw new Error("expected curve type \"weierstrass\" or \"edwards\"");
	if (FpFnLE === void 0) FpFnLE = type === "edwards";
	if (!CURVE || typeof CURVE !== "object") throw new Error(`expected valid ${type} CURVE object`);
	validateObject(curveOpts);
	for (const p of [
		"p",
		"n",
		"h"
	]) {
		const val = CURVE[p];
		if (!(isPosBig(val) && val !== _0n$3)) throw new Error(`CURVE.${p} must be positive bigint`);
	}
	const Fp = createField(CURVE.p, curveOpts.Fp, FpFnLE);
	const Fn = createField(CURVE.n, curveOpts.Fn, FpFnLE);
	const params = [
		"Gx",
		"Gy",
		"a",
		type === "weierstrass" ? "b" : "d"
	];
	for (const p of params) if (!Fp.isValid(CURVE[p])) throw new Error(`CURVE.${p} must be valid field element of CURVE.Fp`);
	CURVE = Object.freeze(Object.assign({}, CURVE));
	return {
		CURVE,
		Fp,
		Fn
	};
}
/**
* @param randomSecretKey - Secret-key generator.
* @param getPublicKey - Public-key derivation helper.
* @returns Keypair generator.
* @example
* Build a `keygen()` helper from existing secret-key and public-key primitives.
*
* ```ts
* import { createKeygen } from '@noble/curves/abstract/curve.js';
* import { p256 } from '@noble/curves/nist.js';
* const keygen = createKeygen(p256.utils.randomSecretKey, p256.getPublicKey);
* const pair = keygen();
* ```
*/
function createKeygen(randomSecretKey, getPublicKey) {
	return function keygen(seed) {
		const secretKey = randomSecretKey(seed);
		return {
			secretKey,
			publicKey: getPublicKey(secretKey)
		};
	};
}
//#endregion
//#region node_modules/@noble/curves/abstract/edwards.js
/**
* Twisted Edwards curve. The formula is: ax² + y² = 1 + dx²y².
* For design rationale of types / exports, see weierstrass module documentation.
* Untwisted Edwards curves exist, but they aren't used in real-world protocols.
* @module
*/
/*! noble-curves - MIT License (c) 2022 Paul Miller (paulmillr.com) */
const _0n$2 = /* @__PURE__ */ BigInt(0);
const _1n$2 = /* @__PURE__ */ BigInt(1);
const _2n$2 = /* @__PURE__ */ BigInt(2);
const _4n = /* @__PURE__ */ BigInt(4);
const _8n$1 = /* @__PURE__ */ BigInt(8);
function isEdValidXY(Fp, CURVE, x, y) {
	const x2 = Fp.sqr(x);
	const y2 = Fp.sqr(y);
	const left = Fp.add(Fp.mul(CURVE.a, x2), y2);
	const right = Fp.add(Fp.ONE, Fp.mul(CURVE.d, Fp.mul(x2, y2)));
	return Fp.eql(left, right);
}
/**
* @param params - Curve parameters. See {@link EdwardsOpts}.
* @param extraOpts - Optional helpers and overrides. See {@link EdwardsExtraOpts}.
* @returns Edwards point constructor. Generator validation here only checks
*   that `(Gx, Gy)` satisfies the affine Edwards equation.
*   RFC 8032 base-point constraints like `B != (0,1)` and `[L]B = 0`
*   are left to the caller's chosen parameters, since eager subgroup
*   validation here adds about 10-15ms to heavyweight imports like ed448.
*   The returned constructor also eagerly marks `Point.BASE` for W=6
*   precompute caching. Some code paths still assume
*   `Fp.BYTES === Fn.BYTES`, so mismatched byte lengths are not fully audited here.
* @throws If the curve parameters or Edwards overrides are invalid. {@link Error}
* @example
* ```ts
* import { edwards } from '@noble/curves/abstract/edwards.js';
* import { jubjub } from '@noble/curves/misc.js';
* // Build a point constructor from explicit curve parameters, then use its base point.
* const Point = edwards(jubjub.Point.CURVE());
* Point.BASE.toHex();
* ```
*/
function edwards(params, extraOpts = {}) {
	validateObject(extraOpts, {}, {}, "extraOpts");
	const opts = extraOpts;
	const validated = createCurveFields("edwards", params, opts, opts.FpFnLE);
	const { Fp, Fn } = validated;
	let CURVE = validated.CURVE;
	const { h: cofactor } = CURVE;
	if (FpLegendre(Fp, CURVE.a) !== 1) throw new Error("edwards: CURVE.a must be a square in Fp for complete addition formulas");
	if (FpLegendre(Fp, CURVE.d) !== -1) throw new Error("edwards: CURVE.d must be a non-square in Fp for complete addition formulas");
	validateObject(opts, {}, {
		uvRatio: "function",
		randomBytes: "function"
	});
	const randomBytes$2 = opts.randomBytes === void 0 ? randomBytes : opts.randomBytes;
	const MASK = _2n$2 << BigInt(Fp.BYTES * 8) - _1n$2;
	function isOdd(n) {
		if (!Fp.isOdd) throw new Error("Field does not have .isOdd()");
		return Fp.isOdd(n);
	}
	const uvRatio = opts.uvRatio === void 0 ? (u, v) => {
		try {
			return {
				isValid: true,
				value: Fp.sqrt(Fp.div(u, v))
			};
		} catch (e) {
			return {
				isValid: false,
				value: _0n$2
			};
		}
	} : opts.uvRatio;
	if (!isEdValidXY(Fp, CURVE, CURVE.Gx, CURVE.Gy)) throw new Error("bad curve params: generator point");
	const mulA = Fp.eql(CURVE.a, Fp.neg(Fp.ONE)) ? (x) => Fp.neg(x) : Fp.eql(CURVE.a, Fp.ONE) ? (x) => x : (x) => Fp.mul(CURVE.a, x);
	/**
	* Asserts coordinate is valid: 0 <= n < MASK.
	* Coordinates >= Fp.ORDER are allowed for zip215.
	*/
	function acoord(title, n, banZero = false) {
		const min = banZero ? _1n$2 : _0n$2;
		aInRange("coordinate " + title, n, min, MASK);
		return n;
	}
	function aedpoint(other) {
		if (!(other instanceof Point)) throw new Error("EdwardsPoint expected");
	}
	class Point {
		static BASE = new Point(CURVE.Gx, CURVE.Gy, Fp.ONE, Fp.mul(CURVE.Gx, CURVE.Gy));
		static ZERO = new Point(Fp.ZERO, Fp.ONE, Fp.ONE, Fp.ZERO);
		static Fp = Fp;
		static Fn = Fn;
		X;
		Y;
		Z;
		T;
		constructor(X, Y, Z, T) {
			this.X = acoord("x", X);
			this.Y = acoord("y", Y);
			this.Z = acoord("z", Z, true);
			this.T = acoord("t", T);
			Object.freeze(this);
		}
		static CURVE() {
			return CURVE;
		}
		/**
		* Create one extended Edwards point from affine coordinates.
		* Does NOT validate that the point is on-curve or torsion-free.
		* Use `.assertValidity()` on adversarial inputs.
		*/
		static fromAffine(p) {
			if (p instanceof Point) throw new Error("extended point not allowed");
			const { x, y } = p || {};
			acoord("x", x);
			acoord("y", y);
			return new Point(x, y, Fp.ONE, Fp.mul(x, y));
		}
		static fromBytes(bytes, zip215 = false) {
			const len = Fp.BYTES;
			const { a, d } = CURVE;
			bytes = copyBytes$1(abytes$1(bytes, len, "point"));
			abool$1(zip215, "zip215");
			const normed = copyBytes$1(bytes);
			const lastByte = bytes[len - 1];
			normed[len - 1] = lastByte & -129;
			const y = bytesToNumberLE(normed);
			const max = zip215 ? MASK : Fp.ORDER;
			aInRange("point.y", y, _0n$2, max);
			const y2 = Fp.sqr(y);
			const u = Fp.sub(y2, Fp.ONE);
			const v = Fp.sub(Fp.mulN(d, y2), a);
			let { isValid, value: x } = uvRatio(u, v);
			if (!isValid) throw new Error("bad point: invalid y coordinate");
			const isXOdd = isOdd(x);
			const isLastByteOdd = (lastByte & 128) !== 0;
			if (!zip215 && Fp.is0(x) && isLastByteOdd) throw new Error("bad point: x=0 and x_0=1");
			if (isLastByteOdd !== isXOdd) x = Fp.neg(x);
			return Point.fromAffine({
				x,
				y
			});
		}
		static fromHex(hex, zip215 = false) {
			return Point.fromBytes(hexToBytes(hex), zip215);
		}
		get x() {
			return this.toAffine().x;
		}
		get y() {
			return this.toAffine().y;
		}
		precompute(windowSize = 6, isLazy = true) {
			wnaf.setWindowSize(this, windowSize);
			if (!isLazy) this.multiply(_2n$2);
			return this;
		}
		assertValidity() {
			const p = this;
			const { a, d } = CURVE;
			if (p.is0()) throw new Error("bad point: ZERO");
			const { X, Y, Z, T } = p;
			const X2 = Fp.sqr(X);
			const Y2 = Fp.sqr(Y);
			const Z2 = Fp.sqr(Z);
			const Z4 = Fp.sqr(Z2);
			const aX2 = Fp.mul(X2, a);
			const left = Fp.mul(Fp.add(aX2, Y2), Z2);
			const right = Fp.add(Z4, Fp.mul(d, Fp.mul(X2, Y2)));
			if (!Fp.eql(left, right)) throw new Error("bad point: equation left != right (1)");
			const XY = Fp.mul(X, Y);
			const ZT = Fp.mul(Z, T);
			if (!Fp.eql(XY, ZT)) throw new Error("bad point: equation left != right (2)");
		}
		equals(other) {
			aedpoint(other);
			const { X: X1, Y: Y1, Z: Z1 } = this;
			const { X: X2, Y: Y2, Z: Z2 } = other;
			const X1Z2 = Fp.mul(X1, Z2);
			const X2Z1 = Fp.mul(X2, Z1);
			const Y1Z2 = Fp.mul(Y1, Z2);
			const Y2Z1 = Fp.mul(Y2, Z1);
			return Fp.eql(X1Z2, X2Z1) && Fp.eql(Y1Z2, Y2Z1);
		}
		is0() {
			return this.equals(Point.ZERO);
		}
		negate() {
			return new Point(Fp.neg(this.X), this.Y, this.Z, Fp.neg(this.T));
		}
		double() {
			const { X: X1, Y: Y1, Z: Z1 } = this;
			const A = Fp.sqr(X1);
			const B = Fp.sqr(Y1);
			const C = Fp.mul(Fp.sqr(Z1), _2n$2);
			const D = mulA(A);
			const x1y1 = Fp.addN(X1, Y1);
			const E = Fp.sub(Fp.subN(Fp.sqr(x1y1), A), B);
			const G = Fp.addN(D, B);
			const F = Fp.subN(G, C);
			const H = Fp.subN(D, B);
			const X3 = Fp.mul(E, F);
			const Y3 = Fp.mul(G, H);
			const T3 = Fp.mul(E, H);
			const Z3 = Fp.mul(F, G);
			return new Point(X3, Y3, Z3, T3);
		}
		add(other) {
			aedpoint(other);
			const { d } = CURVE;
			const { X: X1, Y: Y1, Z: Z1, T: T1 } = this;
			const { X: X2, Y: Y2, Z: Z2, T: T2 } = other;
			const A = Fp.mul(X1, X2);
			const B = Fp.mul(Y1, Y2);
			const C = Fp.mul(Fp.mulN(T1, d), T2);
			const D = Fp.mul(Z1, Z2);
			const E = Fp.sub(Fp.subN(Fp.mulN(Fp.addN(X1, Y1), Fp.addN(X2, Y2)), A), B);
			const F = Fp.subN(D, C);
			const G = Fp.addN(D, C);
			const H = Fp.sub(B, mulA(A));
			const X3 = Fp.mul(E, F);
			const Y3 = Fp.mul(G, H);
			const T3 = Fp.mul(E, H);
			const Z3 = Fp.mul(F, G);
			return new Point(X3, Y3, Z3, T3);
		}
		subtract(other) {
			aedpoint(other);
			return this.add(other.negate());
		}
		multiply(scalar) {
			if (!Fn.isValidNot0(scalar)) throw new RangeError("invalid scalar: expected 1 <= sc < curve.n");
			const { p, f } = wnaf.mulSecret(this, scalar, cofactor, normalize);
			return normalize([p, f])[0];
		}
		multiplyUnsafe(scalar) {
			if (!Fn.isValid(scalar)) throw new RangeError("invalid scalar: expected 0 <= sc < curve.n");
			if (scalar === _0n$2) return Point.ZERO;
			if (this.is0() || scalar === _1n$2) return this;
			return wnaf.mulUnsafe(this, scalar, normalize);
		}
		isSmallOrder() {
			return this.clearCofactor().is0();
		}
		isTorsionFree() {
			return wnaf.mulUnsafe(this, CURVE.n).is0();
		}
		toAffine(invertedZ) {
			const p = this;
			let iz = invertedZ;
			if (iz != null && typeof iz !== "bigint") throw new TypeError("\"invertedZ\" expected bigint, got type=" + typeof iz);
			const { X, Y, Z } = p;
			const is0 = p.is0();
			if (iz == null) iz = is0 ? Fp.create(_8n$1) : Fp.inv(Z);
			const x = Fp.mul(X, iz);
			const y = Fp.mul(Y, iz);
			const zz = Fp.mul(Z, iz);
			if (is0) return {
				x: Fp.ZERO,
				y: Fp.ONE
			};
			if (!Fp.eql(zz, Fp.ONE)) throw new Error("invZ was invalid");
			return {
				x,
				y
			};
		}
		clearCofactor() {
			if (cofactor === _1n$2) return this;
			if (cofactor === _2n$2) return this.double();
			if (cofactor === _4n) return this.double().double();
			if (cofactor === _8n$1) return this.double().double().double();
			return this.multiplyUnsafe(cofactor);
		}
		toBytes() {
			const { x, y } = this.toAffine();
			const bytes = Fp.toBytes(y);
			bytes[bytes.length - 1] |= isOdd(x) ? 128 : 0;
			return bytes;
		}
		toHex() {
			return bytesToHex(this.toBytes());
		}
		toString() {
			return `<Point ${this.is0() ? "ZERO" : this.toHex()}>`;
		}
	}
	const normalize = (points) => normalizeZ(Point, points);
	const wnaf = new ScalarMultiplier(Point, randomBytes$2);
	if (wnaf.bits >= 6) Point.BASE.precompute(6);
	Object.freeze(Point.prototype);
	Object.freeze(Point);
	return Point;
}
//#endregion
//#region node_modules/@noble/curves/abstract/montgomery.js
/**
* Montgomery curve methods. It's not really whole montgomery curve,
* just bunch of very specific methods for X25519 / X448 from
* [RFC 7748](https://www.rfc-editor.org/rfc/rfc7748)
* @module
*/
/*! noble-curves - MIT License (c) 2022 Paul Miller (paulmillr.com) */
const _0n$1 = /* @__PURE__ */ BigInt(0);
const _1n$1 = /* @__PURE__ */ BigInt(1);
const _2n$1 = /* @__PURE__ */ BigInt(2);
/**
* Selector for cswap(): `P` to keep, `P + 1` to swap, chosen by the low bit of `swap`.
* Higher bits are ignored, and `swap` is passed in whole rather than as a {0n, 1n} bit on
* purpose: `P + (swap & _1n)` would short-circuit the addition whenever the bit is clear, which
* is the very leak this construction avoids, one round-trip further down. Subtracting `swap`
* with its low bit cleared keeps every operand full-width instead.
* @param P - Field modulus.
* @param swap - Value whose low bit selects; ignored above that bit.
* @returns `P` when the low bit is clear, `P + 1` when it is set.
*/
function cmask(P, swap) {
	return P + swap - (swap >> _1n$1 << _1n$1);
}
/**
* Swap two field elements when `mask` is `P + 1`, keep them when it is `P`:
*
*   d    = 6P + x_3 - x_2
*   x_2' = d * mask + x_2   (mod P)      x_3' = (x_2 + x_3) - x_2'
*
* The extra `6P * mask` vanishes modulo P, so `mask === P` leaves x_2 and `mask === P + 1`
* leaves x_3. Without the offset, the reduction dividend changes sign with input order and crosses
* BigInt limb boundaries; those classes measured differently on the tested Node/V8 build. For
* canonical inputs, the deliberately left-associative `offset + x_3 - x_2` is between 5P and 7P,
* keeping the dividend positive and in one word-count band for both RFC fields and masks. Six is
* the smallest coefficient `c` for which the shared offset `cP` has that property.
*
* This reduced the tested sign/size timing ratios, but JavaScript BigInt has no constant-time
* contract and the contents of the multiply and remainder still vary. Valid ladder states can
* contain genuine zero coordinates; this construction does not mask those value-shape effects.
* Computing `x_3'` independently as `((6P + x_2 - x_3) * mask + x_3) % P` is more symmetric.
* On the tested Node/V8 build, it reduced the timing difference between keeping `(0, v)` and
* swapping `(v, 0)`—both return `(0, v)`—from about 10%/13% for X25519/X448 to about 3%.
* Successful calls cannot reach that zero-in-the-first-output case. For the case they can reach,
* swapping `(0, v)` and keeping `(v, 0)` both return `(v, 0)`; the difference instead grew from
* about 0.7%/1.1% to 2.7%/2.8%. The extra multiply/remainder also made public
* `getSharedSecret()` about 16% slower. The retained one-remainder form measured about 2.5%
* slower than the prior helper for public X25519 `getSharedSecret()` in the same environment.
* x_3' falls out of the sum, which a swap leaves invariant: no second multiply or reduction is
* needed. Bind `6P` once per field so production and the timing regression exercise the same
* configured helper without paying for the multiplication in every ladder round.
*
* The returned function is called twice per ladder round, so it validates nothing. Both elements
* MUST already be reduced mod P; unreduced input silently corrupts the kept-side output.
* @param P - Field modulus.
* @returns A field-bound swap function taking mask, x_2, and x_3.
*/
function cswap(P) {
	const offset = BigInt(6) * P;
	return (mask, x_2, x_3) => {
		const sum = x_2 + x_3;
		const a = ((offset + x_3 - x_2) * mask + x_2) % P;
		return {
			x_2: a,
			x_3: sum - a
		};
	};
}
function validateOpts(curve) {
	validateObject(curve, {
		P: "bigint",
		type: "string",
		adjustScalarBytes: "function",
		powPminus2: "function"
	}, {
		randomBytes: "function",
		scalarMultBase: "function"
	});
	return Object.freeze({ ...curve });
}
/**
* @param curveDef - Montgomery curve definition.
* @returns ECDH helper namespace.
* @throws If the curve definition or derived shared point is invalid. {@link Error}
* @example
* Build an X25519 helper from curve parameters, then derive one public key.
*
* ```ts
* import { montgomery } from '@noble/curves/abstract/montgomery.js';
* const P = 2n ** 255n - 19n;
* const mod = (num: bigint) => {
*   const out = num % P;
*   return out >= 0n ? out : out + P;
* };
* const pow = (num: bigint, power: bigint) => {
*   let res = 1n;
*   for (; power > 0n; power >>= 1n) {
*     if (power & 1n) res = mod(res * num);
*     num = mod(num * num);
*   }
*   return res;
* };
* const x25519 = montgomery({
*   P,
*   type: 'x25519',
*   adjustScalarBytes(bytes: Uint8Array) {
*     bytes[0] &= 248;
*     bytes[31] &= 127;
*     bytes[31] |= 64;
*     return bytes;
*   },
*   powPminus2(x) {
*     return pow(x, P - 2n);
*   },
* });
* const publicKey = x25519.getPublicKey(new Uint8Array(32).fill(1));
* ```
*/
function montgomery(curveDef) {
	const CURVE = validateOpts(curveDef);
	const { P, type, adjustScalarBytes, powPminus2, randomBytes: rand } = CURVE;
	const mulBaseHook = CURVE.scalarMultBase;
	const is25519 = type === "x25519";
	if (!is25519 && type !== "x448") throw new Error("invalid type");
	const randomBytes_ = rand === void 0 ? randomBytes : rand;
	const montgomeryBits = is25519 ? 255 : 448;
	const swap = cswap(P);
	const fieldLen = is25519 ? 32 : 56;
	const Gu = is25519 ? BigInt(9) : BigInt(5);
	const a24 = is25519 ? BigInt(121665) : BigInt(39081);
	const minScalar = is25519 ? _2n$1 ** BigInt(254) : _2n$1 ** BigInt(447);
	const maxScalar = minScalar + (is25519 ? BigInt(8) * (_2n$1 ** BigInt(251) - _1n$1) : BigInt(4) * (_2n$1 ** BigInt(445) - _1n$1)) + _1n$1;
	const modP = (n) => mod(n, P);
	const GuBytes = encodeU(Gu);
	function encodeU(u) {
		return numberToBytesLE(modP(u), fieldLen);
	}
	function decodeU(u) {
		const _u = copyBytes$1(abytes$1(u, fieldLen, "uCoordinate"));
		if (is25519) _u[31] &= 127;
		return modP(bytesToNumberLE(_u));
	}
	function decodeScalar(scalar) {
		return bytesToNumberLE(adjustScalarBytes(copyBytes$1(abytes$1(scalar, fieldLen, "scalar"))));
	}
	/**
	* u coordinates whose order divides the cofactor, on the curve and on its quadratic twist -
	* the ladder sends every one of them to zero. Same blocklist libsodium and post-CVE-2017-0379
	* Libgcrypt carry. decodeU() reduces mod P first, so the non-canonical encodings P and P + 1
	* collapse onto 0 and 1, and `type` admits no curve beyond these two, so both lists are total.
	*
	* Complete by construction: x-only doubling sends u to (u^2 - 1)^2 / 4u(u^2 + a*u + 1). Order 4
	* therefore needs (u^2 - 1)^2 === 0, i.e. u = +-1; order 2 needs u(u^2 + a*u + 1) === 0, and
	* a^2 - 4 is a non-residue on both curves, leaving u = 0. curve448 stops there (cofactor 4);
	* curve25519 (cofactor 8) adds the two order-8 roots below. Cross-checked by clearing the
	* cofactor with those same doublings over 200k random u: no sixth value exists.
	*/
	const lowOrderU = new Set(is25519 ? [
		_0n$1,
		_1n$1,
		P - _1n$1,
		BigInt("325606250916557431795983626356110631294008115727848805560023387167927233504"),
		BigInt("39382357235489614581723060781553021112529911719440698176882885853963445705823")
	] : [
		_0n$1,
		_1n$1,
		P - _1n$1
	]);
	function scalarMult(scalar, u) {
		const pointU = decodeU(u);
		if (lowOrderU.has(pointU)) throw new Error("invalid private or public key received");
		const pu = montgomeryLadder(pointU, decodeScalar(scalar));
		if (pu === _0n$1) throw new Error("invalid private or public key received");
		return encodeU(pu);
	}
	function scalarMultBase(scalar) {
		if (mulBaseHook === void 0) return scalarMult(scalar, GuBytes);
		const k = decodeScalar(scalar);
		aInRange("scalar", k, minScalar, maxScalar);
		const pu = modP(mulBaseHook(k));
		if (pu === _0n$1) throw new Error("invalid private or public key received");
		return encodeU(pu);
	}
	const getPublicKey = scalarMultBase;
	const getSharedSecret = scalarMult;
	/**
	* Montgomery x-only multiplication ladder for the selected X25519/X448 curve.
	* @param pointU - decoded Montgomery u coordinate for the selected curve
	* @param scalar - decoded clamped scalar by which the point is multiplied
	* @returns resulting Montgomery u coordinate for the selected curve
	*/
	function montgomeryLadder(u, scalar) {
		aInRange("u", u, _0n$1, P);
		aInRange("scalar", scalar, minScalar, maxScalar);
		const k = scalar;
		const x_1 = u;
		let x_2 = _1n$1;
		let z_2 = _0n$1;
		let x_3 = u;
		let z_3 = _1n$1;
		const kx = k ^ k >> _1n$1;
		for (let t = BigInt(montgomeryBits - 1); t >= _0n$1; t--) {
			const mask = cmask(P, kx >> t);
			({x_2, x_3} = swap(mask, x_2, x_3));
			({x_2: z_2, x_3: z_3} = swap(mask, z_2, z_3));
			const A = x_2 + z_2;
			const AA = modP(A * A);
			const B = x_2 - z_2;
			const BB = modP(B * B);
			const E = AA - BB;
			const C = x_3 + z_3;
			const D = x_3 - z_3;
			const DA = modP(D * A);
			const CB = modP(C * B);
			const dacb = DA + CB;
			const da_cb = DA - CB;
			x_3 = modP(dacb * dacb);
			z_3 = modP(x_1 * modP(da_cb * da_cb));
			x_2 = modP(AA * BB);
			z_2 = modP(E * (AA + modP(a24 * E)));
		}
		const mask = cmask(P, k);
		({x_2, x_3} = swap(mask, x_2, x_3));
		({x_2: z_2, x_3: z_3} = swap(mask, z_2, z_3));
		const z2 = powPminus2(z_2);
		return modP(x_2 * z2);
	}
	const lengths = {
		secretKey: fieldLen,
		publicKey: fieldLen,
		seed: fieldLen
	};
	const randomSecretKey = (seed) => {
		seed = seed === void 0 ? randomBytes_(fieldLen) : seed;
		abytes$1(seed, lengths.seed, "seed");
		return seed;
	};
	const utils = { randomSecretKey };
	Object.freeze(lengths);
	Object.freeze(utils);
	return Object.freeze({
		keygen: createKeygen(randomSecretKey, getPublicKey),
		getSharedSecret,
		getPublicKey,
		scalarMult,
		scalarMultBase,
		utils,
		GuBytes: GuBytes.slice(),
		lengths
	});
}
//#endregion
//#region node_modules/@noble/curves/ed25519.js
/*! noble-curves - MIT License (c) 2022 Paul Miller (paulmillr.com) */
const _0n = /* @__PURE__ */ BigInt(0);
const _1n = /* @__PURE__ */ BigInt(1);
const _2n = /* @__PURE__ */ BigInt(2);
const _3n = /* @__PURE__ */ BigInt(3);
const _5n = /* @__PURE__ */ BigInt(5);
const _8n = /* @__PURE__ */ BigInt(8);
const ed25519_CURVE_p = /* @__PURE__ */ BigInt("0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffed");
const ed25519_CURVE = /* @__PURE__ */ (() => ({
	p: ed25519_CURVE_p,
	n: BigInt("0x1000000000000000000000000000000014def9dea2f79cd65812631a5cf5d3ed"),
	h: _8n,
	a: BigInt("0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffec"),
	d: BigInt("0x52036cee2b6ffe738cc740797779e89800700a4d4141d8ab75eb4dca135978a3"),
	Gx: BigInt("0x216936d3cd6e53fec0a4e231fdd6dc5c692cc7609525a7b2c9562d608f25d51a"),
	Gy: BigInt("0x6666666666666666666666666666666666666666666666666666666666666658")
}))();
function ed25519_pow_2_252_3(x) {
	const _10n = BigInt(10), _20n = BigInt(20), _40n = BigInt(40), _80n = BigInt(80);
	const P = ed25519_CURVE_p;
	const b2 = x * x % P * x % P;
	const b5 = pow2(pow2(b2, _2n, P) * b2 % P, _1n, P) * x % P;
	const b10 = pow2(b5, _5n, P) * b5 % P;
	const b20 = pow2(b10, _10n, P) * b10 % P;
	const b40 = pow2(b20, _20n, P) * b20 % P;
	const b80 = pow2(b40, _40n, P) * b40 % P;
	return {
		pow_p_5_8: pow2(pow2(pow2(pow2(b80, _80n, P) * b80 % P, _80n, P) * b80 % P, _10n, P) * b10 % P, _2n, P) * x % P,
		b2
	};
}
function adjustScalarBytes(bytes) {
	bytes[0] &= 248;
	bytes[31] &= 127;
	bytes[31] |= 64;
	return bytes;
}
const ED25519_SQRT_M1 = /* @__PURE__ */ BigInt("19681161376707505956807079304988542015446066515923890162744021073123829784752");
function uvRatio(u, v) {
	const P = ed25519_CURVE_p;
	const v3 = mod(v * v * v, P);
	const pow = ed25519_pow_2_252_3(u * mod(v3 * v3 * v, P)).pow_p_5_8;
	let x = mod(u * v3 * pow, P);
	const vx2 = mod(v * x * x, P);
	const root1 = x;
	const root2 = mod(x * ED25519_SQRT_M1, P);
	const useRoot1 = vx2 === u;
	const useRoot2 = vx2 === mod(-u, P);
	const noRoot = vx2 === mod(-u * ED25519_SQRT_M1, P);
	if (useRoot1) x = root1;
	if (useRoot2 || noRoot) x = root2;
	if (isNegativeLE(x, P)) x = mod(-x, P);
	return {
		isValid: useRoot1 || useRoot2,
		value: x
	};
}
const ed25519_Point = /* @__PURE__ */ edwards(ed25519_CURVE, { uvRatio });
/**
* ECDH using curve25519 aka x25519.
* `getSharedSecret()` rejects low-order peer inputs by default, and seeded
* `keygen(seed)` reuses the provided 32-byte seed buffer instead of copying it.
* @example
* Derive one shared secret between two X25519 peers.
*
* ```js
* import { x25519 } from '@noble/curves/ed25519.js';
* const alice = x25519.keygen();
* const bob = x25519.keygen();
* const alicePublic = x25519.getPublicKey(alice.secretKey);
* const shared = x25519.getSharedSecret(alice.secretKey, bob.publicKey);
* ```
*/
const x25519 = /* @__PURE__ */ (() => {
	const P = ed25519_CURVE_p;
	const powPminus2 = (x) => {
		const { pow_p_5_8, b2 } = ed25519_pow_2_252_3(x);
		return mod(pow2(pow_p_5_8, _3n, P) * b2, P);
	};
	return montgomery({
		P,
		type: "x25519",
		powPminus2,
		adjustScalarBytes,
		scalarMultBase: (k) => {
			const kn = mod(k, ed25519_Point.Fn.ORDER);
			if (kn === _0n) return _0n;
			const p = ed25519_Point.BASE.multiply(kn);
			return mod((p.Z + p.Y) * powPminus2(mod(p.Z - p.Y, P)), P);
		}
	});
})();
//#endregion
//#region node_modules/@noble/ciphers/utils.js
/*! noble-ciphers - MIT License (c) 2023 Paul Miller (paulmillr.com) */
/**
* Checks if something is Uint8Array. Be careful: nodejs Buffer will return true.
* @param a - Value to inspect.
* @returns `true` when the value is a Uint8Array view, including Node's `Buffer`.
* @example
* Guards a value before treating it as raw key material.
*
* ```ts
* isBytes(new Uint8Array());
* ```
*/
function isBytes(a) {
	return a instanceof Uint8Array || ArrayBuffer.isView(a) && a.constructor.name === "Uint8Array" && "BYTES_PER_ELEMENT" in a && a.BYTES_PER_ELEMENT === 1;
}
const atitle = (title) => title ? `"${title}" ` : "";
/**
* Asserts something is boolean.
* @param value - Value to validate.
* @returns The validated boolean.
* @throws On wrong argument types. {@link TypeError}
* @example
* Validates a boolean option before branching on it.
*
* ```ts
* abool(true);
* ```
*/
function abool(value, title = "") {
	if (typeof value !== "boolean") throw new TypeError(atitle(title) + "expected boolean, got type=" + typeof value);
	return value;
}
/**
* Asserts something is a non-negative safe integer.
* @param n - Value to validate.
* @returns The validated number.
* @throws On wrong argument types. {@link TypeError}
* @throws On wrong argument ranges or values. {@link RangeError}
* @example
* Validates a non-negative length or counter.
*
* ```ts
* anumber(1);
* ```
*/
function anumber(n, title = "") {
	if (typeof n !== "number") throw new TypeError(atitle(title) + "expected number, got " + typeof n);
	if (!Number.isSafeInteger(n) || n < 0) throw new RangeError(atitle(title) + "expected integer >= 0, got " + n);
	return n;
}
/**
* Asserts something is Uint8Array.
* @param value - Value to validate.
* @param length - Expected byte length.
* @param title - Optional label used in error messages.
* @returns The validated byte array.
* On Node, `Buffer` is accepted too because it is a Uint8Array view.
* @throws On wrong argument types. {@link TypeError}
* @throws On wrong argument lengths. {@link RangeError}
* @example
* Validates a fixed-length nonce or key buffer.
*
* ```ts
* abytes(new Uint8Array([1, 2]), 2);
* ```
*/
function abytes(value, length, title = "") {
	if (isBytes(value) && (length === void 0 || value.length === length)) return value;
	if (length !== void 0) anumber(length, "length");
	const bytes = isBytes(value);
	const ofLen = length !== void 0 ? ` of length ${length}` : "";
	const got = bytes ? `length=${value.length}` : `type=${typeof value}`;
	const message = atitle(title) + "expected Uint8Array" + ofLen + ", got " + got;
	if (!bytes) throw new TypeError(message);
	throw new RangeError(message);
}
const aobject = (value, label) => {
	if (value === null || typeof value !== "object" || Array.isArray(value)) throw new TypeError(label === "object" ? "expected valid options object" : `"${label}" expected object, got type=${typeof value}`);
};
/**
* Asserts a hash- or MAC-like instance has not been destroyed or finished.
* @param instance - Stateful instance to validate.
* @param checkFinished - Whether to reject finished instances.
* When `false`, only `destroyed` is checked.
* @throws If the hash instance has already been destroyed or finalized. {@link Error}
* @example
* Guards against calling `update()` or `digest()` on a finished hash.
*
* ```ts
* aexists({ destroyed: false, finished: false });
* ```
*/
function aexists(instance, checkFinished = true) {
	if (instance.destroyed) throw new Error("hash was destroyed");
	if (checkFinished && instance.finished) throw new Error("digest() was already called");
}
/**
* Asserts output is a sufficiently-sized byte array.
* @param out - Output buffer to validate.
* @param instance - Hash-like instance providing `outputLen`.
* This is the relaxed `digestInto()`-style contract: output must be at least `outputLen`,
* unlike one-shot cipher helpers elsewhere in the repo that often require exact lengths.
* @throws On wrong argument types. {@link TypeError}
* @throws On wrong output buffer lengths. {@link RangeError}
* @example
* Verifies that a caller-provided output buffer is large enough.
*
* ```ts
* aoutput(new Uint8Array(16), { outputLen: 16 });
* ```
*/
function aoutput(out, instance) {
	abytes(out, void 0, "output");
	const min = instance.outputLen;
	if (!(out.length >= min)) throw new RangeError("\"output\" expected length >= " + min);
}
/**
* Casts a typed-array view to Uint32Array.
* @param arr - Typed-array view to reinterpret.
* @returns Uint32Array view over the same bytes. Callers are expected to provide a
* 4-byte-aligned offset; trailing `1..3` bytes are silently dropped.
* @example
* Views a byte buffer as 32-bit words for block processing.
*
* ```ts
* u32(new Uint8Array(4));
* ```
*/
function u32(arr) {
	return new Uint32Array(arr.buffer, arr.byteOffset, Math.floor(arr.byteLength / 4));
}
/**
* Zeroizes typed arrays in place.
* Warning: JS provides no guarantees.
* @param arrays - Arrays to wipe.
* @example
* Wipes a temporary key buffer after use.
*
* ```ts
* const bytes = new Uint8Array([1]);
* clean(bytes);
* ```
*/
function clean(...arrays) {
	for (let i = 0; i < arrays.length; i++) arrays[i].fill(0);
}
/**
* Creates a DataView for byte-level manipulation.
* @param arr - Typed-array view to wrap.
* @returns DataView over the same bytes.
* @example
* Creates an endian-aware view for length encoding.
*
* ```ts
* createView(new Uint8Array(4));
* ```
*/
function createView(arr) {
	return new DataView(arr.buffer, arr.byteOffset, arr.byteLength);
}
/**
* Whether the current platform is little-endian.
* Most are; some IBM systems are not.
*/
const isLE = /* @__PURE__ */ (() => new Uint8Array(new Uint32Array([287454020]).buffer)[0] === 68)();
/**
* Reverses byte order of one 32-bit word.
* @param word - Unsigned 32-bit word to swap.
* @returns The same word with bytes reversed.
* @example
* Swaps a big-endian word into little-endian byte order.
*
* ```ts
* byteSwap(0x11223344);
* ```
*/
function byteSwap(word) {
	return word << 24 & 4278190080 | word << 8 & 16711680 | word >>> 8 & 65280 | word >>> 24 & 255;
}
/**
* Byte-swaps every word of a Uint32Array in place.
* @param arr - Uint32Array whose words should be swapped.
* @returns The same array after in-place byte swapping.
* @example
* Swaps every 32-bit word in a word-view buffer.
*
* ```ts
* byteSwap32(new Uint32Array([0x11223344]));
* ```
*/
function byteSwap32(arr) {
	for (let i = 0; i < arr.length; i++) arr[i] = byteSwap(arr[i]);
	return arr;
}
/**
* Normalizes a Uint32Array view to the little-endian representation expected by cipher cores.
* @param u - Word view to normalize in place.
* @returns Little-endian normalized word view.
* @example
* Normalizes a word-view buffer before block processing.
*
* ```ts
* swap32IfBE(new Uint32Array([0x11223344]));
* ```
*/
const swap32IfBE = isLE ? (u) => u : byteSwap32;
/**
* Checks if two U8A use same underlying buffer and overlaps.
* This is invalid and can corrupt data.
* @param a - First byte view.
* @param b - Second byte view.
* @returns `true` when the views overlap in memory.
* @example
* Detects whether two slices alias the same backing buffer.
*
* ```ts
* overlapBytes(new Uint8Array(4), new Uint8Array(4));
* ```
*/
function overlapBytes(a, b) {
	if (!a.byteLength || !b.byteLength) return false;
	return a.buffer === b.buffer && a.byteOffset < b.byteOffset + b.byteLength && b.byteOffset < a.byteOffset + a.byteLength;
}
/**
* If input and output overlap and input starts before output, we will overwrite end of input before
* we start processing it, so this is not supported by forward-processing ciphers.
* @param input - Input bytes.
* @param output - Output bytes.
* @throws If the output view would overwrite unread input bytes. {@link Error}
* @example
* Rejects an in-place layout that would overwrite unread input bytes.
*
* ```ts
* const buffer = new Uint8Array(8);
* complexOverlapBytes(buffer.subarray(0, 4), buffer.subarray(2, 6));
* ```
*/
function complexOverlapBytes(input, output) {
	if (overlapBytes(input, output) && input.byteOffset < output.byteOffset) throw new Error("complex overlap of input and output is not supported");
}
/**
* Merges user options into defaults.
* @param defaults - Default option values.
* @param opts - User-provided overrides.
* @returns Combined options object.
* `defaults` is a library-owned mutable object; user-provided `opts` only need to be
* object-shaped, since "plain object" checks reject valid proxy/cross-realm containers.
* The merge mutates `defaults` in place and returns the same object, so direct callers
* should pass a fresh defaults object unless they intentionally want shared state updated.
* @throws If options are missing or not an object. {@link Error}
* @example
* Applies user overrides to the default cipher options.
*
* ```ts
* checkOpts({ rounds: 20 }, { rounds: 8 });
* ```
*/
function checkOpts(defaults, opts) {
	aobject(defaults, "defaults");
	aobject(opts, "opts");
	return Object.assign(defaults, opts);
}
/**
* Compares two byte arrays in kinda constant time once lengths already match.
* @param a - First byte array.
* @param b - Second byte array.
* @returns `true` when the arrays contain the same bytes. Different lengths still return early.
* @example
* Compares an expected authentication tag with the received one.
*
* ```ts
* equalBytes(new Uint8Array([1]), new Uint8Array([1]));
* ```
*/
function equalBytes(a, b) {
	a = abytes(a);
	b = abytes(b);
	if (a.length !== b.length) return false;
	let diff = 0;
	for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
	return diff === 0;
}
/**
* Wraps a keyed MAC constructor into a one-shot helper with `.create()`.
* @param keyLen - Valid probe-key length used to read static metadata once.
* The probe key is only used for `outputLen` / `blockLen`, so callers with several valid key sizes
* can pass any representative size as long as those values stay fixed.
* @param macCons - Keyed MAC constructor or factory.
* @param fromMsg - Optional adapter that derives extra constructor args from the one-shot message.
* @returns Callable MAC helper with `.create()`.
*/
function wrapMacConstructor(keyLen, macCons, fromMsg) {
	const mac = macCons;
	const getArgs = fromMsg || (() => []);
	const macC = (msg, key) => mac(key, ...getArgs(msg)).update(msg).digest();
	const tmp = mac(new Uint8Array(keyLen), ...getArgs(/* @__PURE__ */ new Uint8Array(0)));
	macC.outputLen = tmp.outputLen;
	macC.blockLen = tmp.blockLen;
	macC.create = (key, ...args) => mac(key, ...args);
	return macC;
}
/**
* Wraps a cipher: validates args, ensures encrypt() can only be called once.
* Used internally by the exported cipher constructors.
* Output-buffer support is inferred from the wrapped `encrypt` / `decrypt`
* arity (`fn.length === 2`), so wrapped output-capable methods must use a normal
* second parameter, not a default/rest parameter. AAD support is explicit in
* `params.withAAD`; optional AAD starts after the nonce slot when one is present.
* @__NO_SIDE_EFFECTS__
* @param params - Static cipher metadata. See {@link CipherParams}.
* @param constructor - Cipher constructor.
* @returns Wrapped constructor with validation.
*/
const wrapCipher = (params, constructor) => {
	function wrappedCipher(key, ...args) {
		abytes(key, void 0, "key");
		if (params.nonceLength !== void 0) {
			const nonce = args[0];
			abytes(nonce, params.varSizeNonce ? void 0 : params.nonceLength, "nonce");
		}
		const tagl = params.tagLength;
		const aadStart = params.nonceLength !== void 0 ? 1 : 0;
		if (!params.withAAD) {
			for (let i = aadStart; i < args.length; i++) if (isBytes(args[i])) throw new Error("AAD not supported");
		}
		if (params.withAAD && args[aadStart] !== void 0) abytes(args[aadStart], void 0, "AAD");
		const cipher = constructor(key, ...args);
		const checkOutput = (fnLength, output) => {
			if (output !== void 0) {
				if (fnLength !== 2) throw new Error("cipher output not supported");
				abytes(output, void 0, "output");
			}
		};
		let called = false;
		return {
			encrypt(data, output) {
				if (called) throw new Error("cannot encrypt() twice with same key + nonce");
				called = true;
				abytes(data, void 0, "data");
				checkOutput(cipher.encrypt.length, output);
				return cipher.encrypt(data, output);
			},
			decrypt(data, output) {
				abytes(data, void 0, "data");
				if (tagl && data.length < tagl) throw new Error("\"ciphertext\" expected length >= tagLength=" + tagl);
				checkOutput(cipher.decrypt.length, output);
				return cipher.decrypt(data, output);
			}
		};
	}
	Object.assign(wrappedCipher, params);
	return wrappedCipher;
};
/**
* By default, returns u8a of length.
* When out is available, it checks it for validity and uses it.
* @param expectedLength - Required output length.
* @param out - Optional destination buffer.
* @param onlyAligned - Whether `out` must be 4-byte aligned.
* @returns Output buffer ready for writing.
* @throws On wrong argument types. {@link TypeError}
* @throws If the provided output buffer has the wrong size. {@link RangeError}
* @throws If the provided output buffer has the wrong alignment. {@link Error}
* @example
* Reuses a caller-provided output buffer when lengths match.
*
* ```ts
* getOutput(16, new Uint8Array(16));
* ```
*/
function getOutput(expectedLength, out, onlyAligned = true) {
	if (out === void 0) return new Uint8Array(expectedLength);
	abytes(out, expectedLength, "output");
	if (onlyAligned && !isAligned32(out)) throw new Error("invalid output, must be aligned");
	return out;
}
/**
* Encodes data and AAD lengths into a 16-byte buffer.
* @param dataLength - Data length. Units are caller-defined: GCM passes bit
* lengths, ChaCha20-Poly1305 passes byte lengths — the helper writes the raw values.
* @param aadLength - AAD length, same unit convention as `dataLength`.
* The serialized block is still `aadLength || dataLength`, matching GCM/Poly1305
* conventions even though the helper parameter order is `(dataLength, aadLength)`.
* @param isLE - Whether to encode lengths as little-endian.
* @returns 16-byte length block.
* @throws On wrong argument types passed to the endian validator. {@link TypeError}
* @throws On wrong argument ranges or values. {@link RangeError}
* @example
* Builds the length block appended by GCM and Poly1305.
*
* ```ts
* u64Lengths(16, 8, true);
* ```
*/
function u64Lengths(dataLength, aadLength, isLE) {
	anumber(dataLength);
	anumber(aadLength);
	abool(isLE);
	const num = /* @__PURE__ */ new Uint8Array(16);
	const view = createView(num);
	view.setBigUint64(0, BigInt(aadLength), isLE);
	view.setBigUint64(8, BigInt(dataLength), isLE);
	return num;
}
/**
* Checks whether a byte array is aligned to a 4-byte offset.
* @param bytes - Byte array to inspect.
* @returns `true` when the view is 4-byte aligned.
* @example
* Checks whether a buffer can be safely viewed as Uint32Array.
*
* ```ts
* isAligned32(new Uint8Array(4));
* ```
*/
function isAligned32(bytes) {
	return bytes.byteOffset % 4 === 0;
}
/**
* Copies bytes into a new Uint8Array.
* @param bytes - Bytes to copy.
* @returns Copied byte array.
* @throws On wrong argument types. {@link TypeError}
* @example
* Copies input into an aligned Uint8Array before block processing.
*
* ```ts
* copyBytes(new Uint8Array([1, 2]));
* ```
*/
function copyBytes(bytes) {
	return Uint8Array.from(abytes(bytes));
}
//#endregion
//#region node_modules/@noble/ciphers/_arx.js
/**
* Basic utils for ARX (add-rotate-xor) salsa and chacha ciphers.

RFC8439 requires multi-step cipher stream, where
authKey starts with counter: 0, actual msg with counter: 1.

For this, we need a way to re-use nonce / counter:

const counter = new Uint8Array(4);
chacha(..., counter, ...); // counter is now 1
chacha(..., counter, ...); // counter is now 2

This is complicated:

- 32-bit counters are enough, no need for 64-bit: max ArrayBuffer size in JS is 4GB
- Original papers don't allow mutating counters
- Counter overflow is undefined [^1]
- Idea A: allow providing (nonce | counter) instead of just nonce, re-use it
- Caveat: Cannot be re-used through all cases:
- * chacha has (counter | nonce)
- * xchacha has (nonce16 | counter | nonce16)
- Idea B: separate nonce / counter and provide separate API for counter re-use
- Caveat: there are different counter sizes depending on an algorithm.
- salsa & chacha also differ in structures of key & sigma:
salsa20:      s[0] | k(4) | s[1] | nonce(2) | cnt(2) | s[2] | k(4) | s[3]
chacha:       s(4) | k(8) | cnt(1) | nonce(3)
chacha20orig: s(4) | k(8) | cnt(2) | nonce(2)
- Idea C: helper method such as `setSalsaState(key, nonce, sigma, data)`
- Caveat: we can't re-use counter array

xchacha uses the subkey and remaining 8 byte nonce with ChaCha20 as normal
(prefixed by 4 NUL bytes, since RFC8439 specifies a 12-byte nonce).
Counter overflow is undefined; see {@link https://mailarchive.ietf.org/arch/msg/cfrg/gsOnTJzcbgG6OqD8Sc0GO5aR_tU/ | the CFRG thread}.
Current noble policy is strict non-wrap for the shared 32-bit counter path:
exported ARX ciphers reject initial `0xffffffff` and stop before any implicit
wrap back to zero.
See {@link https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha#appendix-A.2 | the XChaCha appendix} for the extended-nonce construction.

* @module
*/
const encodeStr = (str) => Uint8Array.from(str.split(""), (c) => c.charCodeAt(0));
const sigma16_32 = /* @__PURE__ */ (() => swap32IfBE(u32(encodeStr("expand 16-byte k"))))();
const sigma32_32 = /* @__PURE__ */ (() => swap32IfBE(u32(encodeStr("expand 32-byte k"))))();
/**
* Rotates a 32-bit word left.
* @param a - Input word.
* @param b - Rotation count in bits.
* @returns Rotated 32-bit word.
* @example
* Moves the top byte of `0x12345678` into the low byte position.
* ```ts
* rotl(0x12345678, 8);
* ```
*/
function rotl(a, b) {
	return a << b | a >>> 32 - b;
}
const BLOCK_LEN = 64;
const BLOCK_LEN32 = 16;
const MAX_COUNTER = /* @__PURE__ */ (() => 2 ** 32 - 1)();
const U32_EMPTY = /* @__PURE__ */ Uint32Array.of();
function runCipher(core, sigma, key, nonce, data, output, counter, rounds) {
	const len = data.length;
	const block = new Uint8Array(BLOCK_LEN);
	const b32 = u32(block);
	const isAligned = isLE && isAligned32(data) && isAligned32(output);
	const d32 = isAligned ? u32(data) : U32_EMPTY;
	const o32 = isAligned ? u32(output) : U32_EMPTY;
	if (!isLE) {
		for (let pos = 0; pos < len; counter++) {
			core(sigma, key, nonce, b32, counter, rounds);
			swap32IfBE(b32);
			if (counter >= MAX_COUNTER) throw new Error("arx: counter overflow");
			const take = Math.min(BLOCK_LEN, len - pos);
			for (let j = 0, posj; j < take; j++) {
				posj = pos + j;
				output[posj] = data[posj] ^ block[j];
			}
			pos += take;
		}
		return;
	}
	for (let pos = 0; pos < len; counter++) {
		core(sigma, key, nonce, b32, counter, rounds);
		if (counter >= MAX_COUNTER) throw new Error("arx: counter overflow");
		const take = Math.min(BLOCK_LEN, len - pos);
		if (isAligned && take === BLOCK_LEN) {
			const pos32 = pos / 4;
			if (pos % 4 !== 0) throw new Error("arx: invalid block position");
			for (let j = 0, posj; j < BLOCK_LEN32; j++) {
				posj = pos32 + j;
				o32[posj] = d32[posj] ^ b32[j];
			}
			pos += BLOCK_LEN;
			continue;
		}
		for (let j = 0, posj; j < take; j++) {
			posj = pos + j;
			output[posj] = data[posj] ^ block[j];
		}
		pos += take;
	}
}
/**
* Creates an ARX stream cipher from a 32-bit core permutation.
* Used internally to build the exported Salsa and ChaCha stream ciphers.
* @param core - Core function that fills one keystream block.
* @param opts - Cipher layout and nonce-extension options. See {@link CipherOpts}.
* @returns Stream cipher function over byte arrays.
* @throws If the core callback, key size, counter, or output sizing is invalid. {@link Error}
*/
function createCipher(core, opts) {
	const { allowShortKeys, extendNonceFn, counterLength, counterRight, rounds } = checkOpts({
		allowShortKeys: false,
		counterLength: 8,
		counterRight: false,
		rounds: 20
	}, opts);
	if (typeof core !== "function") throw new Error("core must be a function");
	anumber(counterLength);
	anumber(rounds);
	abool(counterRight);
	abool(allowShortKeys);
	return (key, nonce, data, output, counter = 0) => {
		abytes(key, void 0, "key");
		abytes(nonce, void 0, "nonce");
		abytes(data, void 0, "data");
		const len = data.length;
		const hasOutput = output !== void 0;
		output = getOutput(len, output, false);
		if (hasOutput) complexOverlapBytes(data, output);
		anumber(counter);
		if (counter < 0 || counter >= MAX_COUNTER) throw new Error("arx: counter overflow");
		const toClean = [];
		let l = key.length;
		let k;
		let sigma;
		if (l === 32) {
			toClean.push(k = copyBytes(key));
			sigma = sigma32_32;
		} else if (l === 16 && allowShortKeys) {
			k = /* @__PURE__ */ new Uint8Array(32);
			k.set(key);
			k.set(key, 16);
			sigma = sigma16_32;
			toClean.push(k);
		} else {
			abytes(key, 32, "arx key");
			throw new Error("invalid key size");
		}
		if (!isLE || !isAligned32(nonce)) toClean.push(nonce = copyBytes(nonce));
		let k32 = u32(k);
		if (extendNonceFn) {
			if (nonce.length !== 24) throw new Error("arx: extended nonce must be 24 bytes");
			const n16 = nonce.subarray(0, 16);
			if (isLE) extendNonceFn(sigma, k32, u32(n16), k32);
			else {
				const sigmaRaw = swap32IfBE(Uint32Array.from(sigma));
				extendNonceFn(sigmaRaw, k32, u32(n16), k32);
				clean(sigmaRaw);
				swap32IfBE(k32);
			}
			nonce = nonce.subarray(16);
		} else if (!isLE) swap32IfBE(k32);
		const nonceNcLen = 16 - counterLength;
		if (nonceNcLen !== nonce.length) throw new Error(`arx: nonce must be ${nonceNcLen} or 16 bytes`);
		if (nonceNcLen !== 12) {
			const nc = /* @__PURE__ */ new Uint8Array(12);
			nc.set(nonce, counterRight ? 0 : 12 - nonce.length);
			nonce = nc;
			toClean.push(nonce);
		}
		const n32 = swap32IfBE(u32(nonce));
		try {
			runCipher(core, sigma, k32, n32, data, output, counter, rounds);
			return output;
		} finally {
			clean(...toClean);
		}
	};
}
//#endregion
//#region node_modules/@noble/ciphers/_poly1305.js
/**
* Poly1305 ({@link https://cr.yp.to/mac/poly1305-20050329.pdf | PDF},
* {@link https://en.wikipedia.org/wiki/Poly1305 | wiki})
* is a fast and parallel secret-key message-authentication code suitable for
* a wide variety of applications. It was standardized in
* {@link https://www.rfc-editor.org/rfc/rfc8439 | RFC 8439} and is now used in TLS 1.3.
*
* Polynomial MACs are not perfect for every situation:
* they lack Random Key Robustness: the MAC can be forged, and can't be used in PAKE schemes.
* See {@link https://keymaterial.net/2020/09/07/invisible-salamanders-in-aes-gcm-siv/ | the invisible salamanders attack writeup}.
* To combat invisible salamanders, `hash(key)` can be included in ciphertext,
* however, this would violate ciphertext indistinguishability:
* an attacker would know which key was used - so `HKDF(key, i)`
* could be used instead.
*
* Check out the {@link https://cr.yp.to/mac.html | original website}.
* Based on public-domain {@link https://github.com/floodyberry/poly1305-donna | poly1305-donna}.
* @module
*/
function u8to16(a, i) {
	return a[i++] & 255 | (a[i++] & 255) << 8;
}
/**
* Incremental Poly1305 MAC state.
* Prefer `poly1305()` for one-shot use.
* @param key - 32-byte Poly1305 one-time key.
* @example
* Feeds one chunk into an incremental Poly1305 state with a fresh one-time key.
*
* ```ts
* import { Poly1305 } from '@noble/ciphers/_poly1305.js';
* import { randomBytes } from '@noble/ciphers/utils.js';
* const key = randomBytes(32);
* const mac = new Poly1305(key);
* mac.update(new Uint8Array([1, 2, 3]));
* mac.digest();
* ```
*/
var Poly1305 = class {
	blockLen = 16;
	outputLen = 16;
	buffer = /* @__PURE__ */ new Uint8Array(16);
	r = /* @__PURE__ */ new Uint16Array(10);
	h = /* @__PURE__ */ new Uint16Array(10);
	pad = /* @__PURE__ */ new Uint16Array(8);
	pos = 0;
	finished = false;
	destroyed = false;
	constructor(key) {
		key = copyBytes(abytes(key, 32, "key"));
		const t0 = u8to16(key, 0);
		const t1 = u8to16(key, 2);
		const t2 = u8to16(key, 4);
		const t3 = u8to16(key, 6);
		const t4 = u8to16(key, 8);
		const t5 = u8to16(key, 10);
		const t6 = u8to16(key, 12);
		const t7 = u8to16(key, 14);
		this.r[0] = t0 & 8191;
		this.r[1] = (t0 >>> 13 | t1 << 3) & 8191;
		this.r[2] = (t1 >>> 10 | t2 << 6) & 7939;
		this.r[3] = (t2 >>> 7 | t3 << 9) & 8191;
		this.r[4] = (t3 >>> 4 | t4 << 12) & 255;
		this.r[5] = t4 >>> 1 & 8190;
		this.r[6] = (t4 >>> 14 | t5 << 2) & 8191;
		this.r[7] = (t5 >>> 11 | t6 << 5) & 8065;
		this.r[8] = (t6 >>> 8 | t7 << 8) & 8191;
		this.r[9] = t7 >>> 5 & 127;
		for (let i = 0; i < 8; i++) this.pad[i] = u8to16(key, 16 + 2 * i);
	}
	process(data, offset, isLast = false) {
		const hibit = isLast ? 0 : 2048;
		const { h, r } = this;
		const r0 = r[0];
		const r1 = r[1];
		const r2 = r[2];
		const r3 = r[3];
		const r4 = r[4];
		const r5 = r[5];
		const r6 = r[6];
		const r7 = r[7];
		const r8 = r[8];
		const r9 = r[9];
		const t0 = u8to16(data, offset + 0);
		const t1 = u8to16(data, offset + 2);
		const t2 = u8to16(data, offset + 4);
		const t3 = u8to16(data, offset + 6);
		const t4 = u8to16(data, offset + 8);
		const t5 = u8to16(data, offset + 10);
		const t6 = u8to16(data, offset + 12);
		const t7 = u8to16(data, offset + 14);
		let h0 = h[0] + (t0 & 8191);
		let h1 = h[1] + ((t0 >>> 13 | t1 << 3) & 8191);
		let h2 = h[2] + ((t1 >>> 10 | t2 << 6) & 8191);
		let h3 = h[3] + ((t2 >>> 7 | t3 << 9) & 8191);
		let h4 = h[4] + ((t3 >>> 4 | t4 << 12) & 8191);
		let h5 = h[5] + (t4 >>> 1 & 8191);
		let h6 = h[6] + ((t4 >>> 14 | t5 << 2) & 8191);
		let h7 = h[7] + ((t5 >>> 11 | t6 << 5) & 8191);
		let h8 = h[8] + ((t6 >>> 8 | t7 << 8) & 8191);
		let h9 = h[9] + (t7 >>> 5 | hibit);
		let c = 0;
		let d0 = c + h0 * r0 + h1 * (5 * r9) + h2 * (5 * r8) + h3 * (5 * r7) + h4 * (5 * r6);
		c = d0 >>> 13;
		d0 &= 8191;
		d0 += h5 * (5 * r5) + h6 * (5 * r4) + h7 * (5 * r3) + h8 * (5 * r2) + h9 * (5 * r1);
		c += d0 >>> 13;
		d0 &= 8191;
		let d1 = c + h0 * r1 + h1 * r0 + h2 * (5 * r9) + h3 * (5 * r8) + h4 * (5 * r7);
		c = d1 >>> 13;
		d1 &= 8191;
		d1 += h5 * (5 * r6) + h6 * (5 * r5) + h7 * (5 * r4) + h8 * (5 * r3) + h9 * (5 * r2);
		c += d1 >>> 13;
		d1 &= 8191;
		let d2 = c + h0 * r2 + h1 * r1 + h2 * r0 + h3 * (5 * r9) + h4 * (5 * r8);
		c = d2 >>> 13;
		d2 &= 8191;
		d2 += h5 * (5 * r7) + h6 * (5 * r6) + h7 * (5 * r5) + h8 * (5 * r4) + h9 * (5 * r3);
		c += d2 >>> 13;
		d2 &= 8191;
		let d3 = c + h0 * r3 + h1 * r2 + h2 * r1 + h3 * r0 + h4 * (5 * r9);
		c = d3 >>> 13;
		d3 &= 8191;
		d3 += h5 * (5 * r8) + h6 * (5 * r7) + h7 * (5 * r6) + h8 * (5 * r5) + h9 * (5 * r4);
		c += d3 >>> 13;
		d3 &= 8191;
		let d4 = c + h0 * r4 + h1 * r3 + h2 * r2 + h3 * r1 + h4 * r0;
		c = d4 >>> 13;
		d4 &= 8191;
		d4 += h5 * (5 * r9) + h6 * (5 * r8) + h7 * (5 * r7) + h8 * (5 * r6) + h9 * (5 * r5);
		c += d4 >>> 13;
		d4 &= 8191;
		let d5 = c + h0 * r5 + h1 * r4 + h2 * r3 + h3 * r2 + h4 * r1;
		c = d5 >>> 13;
		d5 &= 8191;
		d5 += h5 * r0 + h6 * (5 * r9) + h7 * (5 * r8) + h8 * (5 * r7) + h9 * (5 * r6);
		c += d5 >>> 13;
		d5 &= 8191;
		let d6 = c + h0 * r6 + h1 * r5 + h2 * r4 + h3 * r3 + h4 * r2;
		c = d6 >>> 13;
		d6 &= 8191;
		d6 += h5 * r1 + h6 * r0 + h7 * (5 * r9) + h8 * (5 * r8) + h9 * (5 * r7);
		c += d6 >>> 13;
		d6 &= 8191;
		let d7 = c + h0 * r7 + h1 * r6 + h2 * r5 + h3 * r4 + h4 * r3;
		c = d7 >>> 13;
		d7 &= 8191;
		d7 += h5 * r2 + h6 * r1 + h7 * r0 + h8 * (5 * r9) + h9 * (5 * r8);
		c += d7 >>> 13;
		d7 &= 8191;
		let d8 = c + h0 * r8 + h1 * r7 + h2 * r6 + h3 * r5 + h4 * r4;
		c = d8 >>> 13;
		d8 &= 8191;
		d8 += h5 * r3 + h6 * r2 + h7 * r1 + h8 * r0 + h9 * (5 * r9);
		c += d8 >>> 13;
		d8 &= 8191;
		let d9 = c + h0 * r9 + h1 * r8 + h2 * r7 + h3 * r6 + h4 * r5;
		c = d9 >>> 13;
		d9 &= 8191;
		d9 += h5 * r4 + h6 * r3 + h7 * r2 + h8 * r1 + h9 * r0;
		c += d9 >>> 13;
		d9 &= 8191;
		c = (c << 2) + c | 0;
		c = c + d0 | 0;
		d0 = c & 8191;
		c = c >>> 13;
		d1 += c;
		h[0] = d0;
		h[1] = d1;
		h[2] = d2;
		h[3] = d3;
		h[4] = d4;
		h[5] = d5;
		h[6] = d6;
		h[7] = d7;
		h[8] = d8;
		h[9] = d9;
	}
	finalize() {
		const { h, pad } = this;
		const g = /* @__PURE__ */ new Uint16Array(10);
		let c = h[1] >>> 13;
		h[1] &= 8191;
		for (let i = 2; i < 10; i++) {
			h[i] += c;
			c = h[i] >>> 13;
			h[i] &= 8191;
		}
		h[0] += c * 5;
		c = h[0] >>> 13;
		h[0] &= 8191;
		h[1] += c;
		c = h[1] >>> 13;
		h[1] &= 8191;
		h[2] += c;
		g[0] = h[0] + 5;
		c = g[0] >>> 13;
		g[0] &= 8191;
		for (let i = 1; i < 10; i++) {
			g[i] = h[i] + c;
			c = g[i] >>> 13;
			g[i] &= 8191;
		}
		g[9] -= 8192;
		let mask = (c ^ 1) - 1;
		for (let i = 0; i < 10; i++) g[i] &= mask;
		mask = ~mask;
		for (let i = 0; i < 10; i++) h[i] = h[i] & mask | g[i];
		h[0] = (h[0] | h[1] << 13) & 65535;
		h[1] = (h[1] >>> 3 | h[2] << 10) & 65535;
		h[2] = (h[2] >>> 6 | h[3] << 7) & 65535;
		h[3] = (h[3] >>> 9 | h[4] << 4) & 65535;
		h[4] = (h[4] >>> 12 | h[5] << 1 | h[6] << 14) & 65535;
		h[5] = (h[6] >>> 2 | h[7] << 11) & 65535;
		h[6] = (h[7] >>> 5 | h[8] << 8) & 65535;
		h[7] = (h[8] >>> 8 | h[9] << 5) & 65535;
		let f = h[0] + pad[0];
		h[0] = f & 65535;
		for (let i = 1; i < 8; i++) {
			f = (h[i] + pad[i] | 0) + (f >>> 16) | 0;
			h[i] = f & 65535;
		}
		clean(g);
	}
	update(data) {
		aexists(this);
		abytes(data);
		data = copyBytes(data);
		const { buffer, blockLen } = this;
		const len = data.length;
		for (let pos = 0; pos < len;) {
			const take = Math.min(blockLen - this.pos, len - pos);
			if (take === blockLen) {
				for (; blockLen <= len - pos; pos += blockLen) this.process(data, pos);
				continue;
			}
			buffer.set(data.subarray(pos, pos + take), this.pos);
			this.pos += take;
			pos += take;
			if (this.pos === blockLen) {
				this.process(buffer, 0, false);
				this.pos = 0;
			}
		}
		return this;
	}
	destroy() {
		this.destroyed = true;
		clean(this.h, this.r, this.buffer, this.pad);
	}
	digestInto(out) {
		aexists(this);
		aoutput(out, this);
		this.finished = true;
		const { buffer, h } = this;
		let { pos } = this;
		if (pos) {
			buffer[pos++] = 1;
			for (; pos < 16; pos++) buffer[pos] = 0;
			this.process(buffer, 0, true);
		}
		this.finalize();
		let opos = 0;
		for (let i = 0; i < 8; i++) {
			out[opos++] = h[i] >>> 0;
			out[opos++] = h[i] >>> 8;
		}
	}
	digest() {
		const { buffer, outputLen } = this;
		this.digestInto(buffer);
		const res = buffer.slice(0, outputLen);
		this.destroy();
		return res;
	}
};
/**
* Poly1305 MAC from RFC 8439.
* @param msg - Message bytes to authenticate.
* @param key - 32-byte Poly1305 one-time key.
* @returns 16-byte authentication tag.
* @example
* Authenticates one message with a one-shot Poly1305 call and a fresh key.
*
* ```ts
* import { poly1305 } from '@noble/ciphers/_poly1305.js';
* import { randomBytes } from '@noble/ciphers/utils.js';
* const key = randomBytes(32);
* poly1305(new Uint8Array(), key);
* ```
*/
const poly1305 = /* @__PURE__ */ wrapMacConstructor(32, (key) => new Poly1305(key));
//#endregion
//#region node_modules/@noble/ciphers/chacha.js
/**
* ChaCha stream cipher, released
* in 2008. Developed after Salsa20, ChaCha aims to increase diffusion per round.
* It was standardized in
* {@link https://www.rfc-editor.org/rfc/rfc8439 | RFC 8439} and
* is now used in TLS 1.3.
*
* {@link https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha | XChaCha20}
* extended-nonce variant is also provided. Similar to XSalsa, it's safe to use with
* randomly-generated nonces.
*
* Check out
* {@link http://cr.yp.to/chacha/chacha-20080128.pdf | PDF},
* {@link https://en.wikipedia.org/wiki/Salsa20 | wiki}, and
* {@link https://cr.yp.to/chacha.html | website}.
*
* @module
*/
/**
* ChaCha core function. Uses an unrolled loop (chachaCore, hchacha) - 4x
* faster than a simple loop, but larger & harder to read. A simple-loop
* reference version lives in `test/misc/micro-ciphers.ts`;
* `test/arx.test.ts` keeps the two aligned.
* The specific implementation is selected in `createCipher` below.
*/
/** RFC 8439 §2.3 block core for `state = constants | key | counter | nonce`. */
function chachaCore(s, k, n, out, cnt, rounds = 20) {
	let y00 = s[0], y01 = s[1], y02 = s[2], y03 = s[3], y04 = k[0], y05 = k[1], y06 = k[2], y07 = k[3], y08 = k[4], y09 = k[5], y10 = k[6], y11 = k[7], y12 = cnt, y13 = n[0], y14 = n[1], y15 = n[2];
	let x00 = y00, x01 = y01, x02 = y02, x03 = y03, x04 = y04, x05 = y05, x06 = y06, x07 = y07, x08 = y08, x09 = y09, x10 = y10, x11 = y11, x12 = y12, x13 = y13, x14 = y14, x15 = y15;
	for (let r = 0; r < rounds; r += 2) {
		x00 = x00 + x04 | 0;
		x12 = rotl(x12 ^ x00, 16);
		x08 = x08 + x12 | 0;
		x04 = rotl(x04 ^ x08, 12);
		x00 = x00 + x04 | 0;
		x12 = rotl(x12 ^ x00, 8);
		x08 = x08 + x12 | 0;
		x04 = rotl(x04 ^ x08, 7);
		x01 = x01 + x05 | 0;
		x13 = rotl(x13 ^ x01, 16);
		x09 = x09 + x13 | 0;
		x05 = rotl(x05 ^ x09, 12);
		x01 = x01 + x05 | 0;
		x13 = rotl(x13 ^ x01, 8);
		x09 = x09 + x13 | 0;
		x05 = rotl(x05 ^ x09, 7);
		x02 = x02 + x06 | 0;
		x14 = rotl(x14 ^ x02, 16);
		x10 = x10 + x14 | 0;
		x06 = rotl(x06 ^ x10, 12);
		x02 = x02 + x06 | 0;
		x14 = rotl(x14 ^ x02, 8);
		x10 = x10 + x14 | 0;
		x06 = rotl(x06 ^ x10, 7);
		x03 = x03 + x07 | 0;
		x15 = rotl(x15 ^ x03, 16);
		x11 = x11 + x15 | 0;
		x07 = rotl(x07 ^ x11, 12);
		x03 = x03 + x07 | 0;
		x15 = rotl(x15 ^ x03, 8);
		x11 = x11 + x15 | 0;
		x07 = rotl(x07 ^ x11, 7);
		x00 = x00 + x05 | 0;
		x15 = rotl(x15 ^ x00, 16);
		x10 = x10 + x15 | 0;
		x05 = rotl(x05 ^ x10, 12);
		x00 = x00 + x05 | 0;
		x15 = rotl(x15 ^ x00, 8);
		x10 = x10 + x15 | 0;
		x05 = rotl(x05 ^ x10, 7);
		x01 = x01 + x06 | 0;
		x12 = rotl(x12 ^ x01, 16);
		x11 = x11 + x12 | 0;
		x06 = rotl(x06 ^ x11, 12);
		x01 = x01 + x06 | 0;
		x12 = rotl(x12 ^ x01, 8);
		x11 = x11 + x12 | 0;
		x06 = rotl(x06 ^ x11, 7);
		x02 = x02 + x07 | 0;
		x13 = rotl(x13 ^ x02, 16);
		x08 = x08 + x13 | 0;
		x07 = rotl(x07 ^ x08, 12);
		x02 = x02 + x07 | 0;
		x13 = rotl(x13 ^ x02, 8);
		x08 = x08 + x13 | 0;
		x07 = rotl(x07 ^ x08, 7);
		x03 = x03 + x04 | 0;
		x14 = rotl(x14 ^ x03, 16);
		x09 = x09 + x14 | 0;
		x04 = rotl(x04 ^ x09, 12);
		x03 = x03 + x04 | 0;
		x14 = rotl(x14 ^ x03, 8);
		x09 = x09 + x14 | 0;
		x04 = rotl(x04 ^ x09, 7);
	}
	let oi = 0;
	out[oi++] = y00 + x00 | 0;
	out[oi++] = y01 + x01 | 0;
	out[oi++] = y02 + x02 | 0;
	out[oi++] = y03 + x03 | 0;
	out[oi++] = y04 + x04 | 0;
	out[oi++] = y05 + x05 | 0;
	out[oi++] = y06 + x06 | 0;
	out[oi++] = y07 + x07 | 0;
	out[oi++] = y08 + x08 | 0;
	out[oi++] = y09 + x09 | 0;
	out[oi++] = y10 + x10 | 0;
	out[oi++] = y11 + x11 | 0;
	out[oi++] = y12 + x12 | 0;
	out[oi++] = y13 + x13 | 0;
	out[oi++] = y14 + x14 | 0;
	out[oi++] = y15 + x15 | 0;
}
/**
* hchacha hashes key and nonce into key' and nonce' for xchacha20.
* Algorithmically identical to `hchacha_small` from `test/misc/micro-ciphers.ts`,
* but this exported path normalizes word order on big-endian hosts.
* Reuses `chachaCore` and subtracts its feed-forward, keeping the hot per-block
* path untouched.
* @param s - Sigma constants as 32-bit words.
* @param k - Key words.
* @param i - Nonce-prefix words.
* @param out - Output buffer for the derived subkey.
* @example
* Derives the XChaCha subkey from sigma, key, and nonce-prefix words.
*
* ```ts
* const sigma = new Uint32Array(4);
* const key = new Uint32Array(8);
* const nonce = new Uint32Array(4);
* const out = new Uint32Array(8);
* hchacha(sigma, key, nonce, out);
* ```
*/
function hchacha(s, k, i, out) {
	const s2 = isLE ? s : swap32IfBE(s.slice(0, 4));
	const k2 = isLE ? k : swap32IfBE(k.slice(0, 8));
	const i2 = isLE ? i : swap32IfBE(i.slice(0, 4));
	const t = /* @__PURE__ */ new Uint32Array(16);
	chachaCore(s2, k2, i2.subarray(1), t, i2[0]);
	let oi = 0;
	out[oi++] = t[0] - s2[0] | 0;
	out[oi++] = t[1] - s2[1] | 0;
	out[oi++] = t[2] - s2[2] | 0;
	out[oi++] = t[3] - s2[3] | 0;
	out[oi++] = t[12] - i2[0] | 0;
	out[oi++] = t[13] - i2[1] | 0;
	out[oi++] = t[14] - i2[2] | 0;
	out[oi++] = t[15] - i2[3] | 0;
	swap32IfBE(out);
	if (!isLE) clean(s2, k2, i2);
	clean(t);
}
/**
* XChaCha eXtended-nonce ChaCha. With 24-byte nonce, it's safe to make it random (CSPRNG).
* See {@link https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha | the IRTF draft}.
* The nonce/counter layout still reserves 8 counter bytes internally, but the shared public
* `counter` argument follows noble's strict non-wrapping 32-bit policy. See `src/_arx.ts`
* near `MAX_COUNTER` for the full counter-policy rationale.
* @param key - 32-byte key.
* @param nonce - 24-byte extended nonce.
* @param data - Input bytes to xor with the keystream.
* @param output - Optional destination buffer.
* @param counter - Initial block counter.
* @returns Encrypted or decrypted bytes.
* @example
* Encrypts bytes with XChaCha20 using a fresh key and random 24-byte nonce.
*
* ```ts
* import { xchacha20 } from '@noble/ciphers/chacha.js';
* import { randomBytes } from '@noble/ciphers/utils.js';
* const key = randomBytes(32);
* const nonce = randomBytes(24);
* xchacha20(key, nonce, new Uint8Array(4));
* ```
*/
const xchacha20 = /* @__PURE__ */ createCipher(chachaCore, {
	counterRight: false,
	counterLength: 8,
	extendNonceFn: hchacha,
	allowShortKeys: false
});
const ZEROS16 = /* @__PURE__ */ new Uint8Array(16);
const updatePadded = (h, msg) => {
	h.update(msg);
	const leftover = msg.length % 16;
	if (leftover) h.update(ZEROS16.subarray(leftover));
};
const ZEROS32 = /* @__PURE__ */ new Uint8Array(32);
function computeTag(fn, key, nonce, ciphertext, AAD) {
	if (AAD !== void 0) abytes(AAD, void 0, "AAD");
	const authKey = fn(key, nonce, ZEROS32);
	const lengths = u64Lengths(ciphertext.length, AAD ? AAD.length : 0, true);
	const h = poly1305.create(authKey);
	if (AAD) updatePadded(h, AAD);
	updatePadded(h, ciphertext);
	h.update(lengths);
	const res = h.digest();
	clean(authKey, lengths);
	return res;
}
/**
* AEAD algorithm from RFC 8439.
* Salsa20 and chacha (RFC 8439) use poly1305 differently.
* We could have composed them, but it's hard because of authKey:
* In salsa20, authKey changes position in salsa stream.
* In chacha, authKey can't be computed inside computeTag, it modifies the counter.
*/
const _poly1305_aead = (xorStream) => (key, nonce, AAD) => {
	const tagLength = 16;
	return {
		encrypt(plaintext, output) {
			const plength = plaintext.length;
			output = getOutput(plength + tagLength, output, false);
			output.set(plaintext);
			const oPlain = output.subarray(0, -16);
			xorStream(key, nonce, oPlain, oPlain, 1);
			const tag = computeTag(xorStream, key, nonce, oPlain, AAD);
			output.set(tag, plength);
			clean(tag);
			return output;
		},
		decrypt(ciphertext, output) {
			output = getOutput(ciphertext.length - tagLength, output, false);
			const data = ciphertext.subarray(0, -16);
			const passedTag = ciphertext.subarray(-16);
			const tag = computeTag(xorStream, key, nonce, data, AAD);
			if (!equalBytes(passedTag, tag)) {
				clean(tag);
				throw new Error("invalid tag");
			}
			output.set(ciphertext.subarray(0, -16));
			xorStream(key, nonce, output, output, 1);
			clean(tag);
			return output;
		}
	};
};
/**
* XChaCha20-Poly1305 extended-nonce chacha.
*
* Can be safely used with random nonces (CSPRNG).
* See {@link https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha | the IRTF draft}.
* @param key - 32-byte key.
* @param nonce - 24-byte nonce.
* @param AAD - Additional authenticated data.
* @returns AEAD cipher instance.
* @example
* Encrypts and authenticates plaintext with a fresh key and random 24-byte nonce.
*
* ```ts
* import { xchacha20poly1305 } from '@noble/ciphers/chacha.js';
* import { randomBytes } from '@noble/ciphers/utils.js';
* const key = randomBytes(32);
* const nonce = randomBytes(24);
* const aad = new TextEncoder().encode('session metadata');
* const cipher = xchacha20poly1305(key, nonce, aad);
* cipher.encrypt(new Uint8Array([1, 2, 3]));
* ```
*/
const xchacha20poly1305 = /* @__PURE__ */ wrapCipher({
	blockSize: 64,
	nonceLength: 24,
	tagLength: 16,
	withAAD: true
}, /* @__PURE__ */ _poly1305_aead(xchacha20));
//#endregion
//#region node_modules/@noble/hashes/hmac.js
/**
* HMAC: RFC2104 message authentication code.
* @module
*/
/**
* Internal class for HMAC.
* Accepts any byte key, although RFC 2104 §3 recommends keys at least
* `HashLen` bytes long.
*/
var _HMAC = class {
	oHash;
	iHash;
	blockLen;
	outputLen;
	canXOF = false;
	finished = false;
	destroyed = false;
	constructor(hash, key) {
		ahash(hash);
		abytes$2(key, void 0, "key");
		this.iHash = hash.create();
		if (typeof this.iHash.update !== "function") throw new Error("expected Hash instance");
		this.blockLen = this.iHash.blockLen;
		this.outputLen = this.iHash.outputLen;
		const blockLen = this.blockLen;
		const pad = new Uint8Array(blockLen);
		pad.set(key.length > blockLen ? hash.create().update(key).digest() : key);
		for (let i = 0; i < pad.length; i++) pad[i] ^= 54;
		this.iHash.update(pad);
		this.oHash = hash.create();
		for (let i = 0; i < pad.length; i++) pad[i] ^= 106;
		this.oHash.update(pad);
		clean$1(pad);
	}
	update(buf) {
		aexists$1(this);
		this.iHash.update(buf);
		return this;
	}
	digestInto(out) {
		aexists$1(this);
		aoutput$1(out, this);
		this.finished = true;
		const buf = out.subarray(0, this.outputLen);
		this.iHash.digestInto(buf);
		this.oHash.update(buf);
		this.oHash.digestInto(buf);
		this.destroy();
	}
	digest() {
		const out = new Uint8Array(this.oHash.outputLen);
		this.digestInto(out);
		return out;
	}
	_cloneInto(to) {
		to ||= Object.create(Object.getPrototypeOf(this), {});
		const { oHash, iHash, finished, destroyed, blockLen, outputLen, canXOF } = this;
		to = to;
		to.finished = finished;
		to.destroyed = destroyed;
		to.blockLen = blockLen;
		to.outputLen = outputLen;
		to.canXOF = canXOF;
		to.oHash = oHash._cloneInto(to.oHash);
		to.iHash = iHash._cloneInto(to.iHash);
		return to;
	}
	clone() {
		return this._cloneInto();
	}
	destroy() {
		this.destroyed = true;
		this.oHash.destroy();
		this.iHash.destroy();
	}
};
const hmac = /* @__PURE__ */ (() => {
	const hmac_ = ((hash, key, message) => new _HMAC(hash, key).update(message).digest());
	hmac_.create = (hash, key) => new _HMAC(hash, key);
	return hmac_;
})();
//#endregion
//#region node_modules/@noble/hashes/hkdf.js
/**
* HKDF (RFC 5869): extract + expand in one step.
* See {@link https://soatok.blog/2021/11/17/understanding-hkdf/}.
* @module
*/
const HKDF_COUNTER = /* @__PURE__ */ Uint8Array.of(0);
const EMPTY_BUFFER = /* @__PURE__ */ Uint8Array.of();
/**
* HKDF-expand from the spec. The most important part. `HKDF-Expand(PRK, info, L) -> OKM`
* @param hash - hash function that would be used (e.g. sha256)
* @param prk - a pseudorandom key of at least HashLen octets
*   (usually, the output from the extract step)
* @param info - optional context and application specific information (can be a zero-length string)
* @param length - length of output keying material in bytes.
*   RFC 5869 §2.3 allows `0..255*HashLen`, so `0` returns an empty OKM.
* @param _recycled - Internal destroyed extract hashes owned by the combined `hkdf()` call.
* @returns Output keying material with the requested length.
* @throws If the requested output length exceeds the HKDF limit
*   for the selected hash. {@link Error}
* @example
* Run the HKDF expand step.
* ```ts
* import { expand } from '@noble/hashes/hkdf.js';
* import { sha256 } from '@noble/hashes/sha2.js';
* expand(sha256, new Uint8Array(32), new Uint8Array([1, 2, 3]), 16);
* ```
*/
function expand(hash, prk, info, length = 32, _recycled) {
	ahash(hash);
	anumber$2(length, "length");
	abytes$2(prk, void 0, "prk");
	const olen = hash.outputLen;
	if (prk.length < olen) throw new Error("\"prk\" must be at least HashLen octets");
	if (length > 255 * olen) throw new Error("Length must be <= 255*HashLen");
	const blocks = Math.ceil(length / olen);
	if (info === void 0) info = EMPTY_BUFFER;
	else abytes$2(info, void 0, "info");
	if (!blocks) {
		if (_recycled) clean$1(prk);
		return /* @__PURE__ */ new Uint8Array();
	}
	const okm = _recycled && blocks === 1 ? prk : new Uint8Array(blocks * olen);
	const { iHash, oHash } = hmac.create(hash, prk);
	const T = _recycled ? prk : new Uint8Array(olen);
	const worker = blocks > 1 ? _recycled?.iHash || hash.create() : void 0;
	for (let counter = 0; counter < blocks - 1; counter++) {
		HKDF_COUNTER[0] = counter + 1;
		const iWork = iHash._cloneInto(worker);
		if (counter) iWork.update(T);
		iWork.update(info).update(HKDF_COUNTER).digestInto(T);
		oHash._cloneInto(worker).update(T).digestInto(T);
		okm.set(T, olen * counter);
	}
	HKDF_COUNTER[0] = blocks;
	if (blocks > 1) iHash.update(T);
	iHash.update(info).update(HKDF_COUNTER).digestInto(T);
	oHash.update(T).digestInto(T);
	okm.set(T, olen * (blocks - 1));
	iHash.destroy();
	oHash.destroy();
	worker?.destroy();
	if (T !== okm) clean$1(T);
	clean$1(HKDF_COUNTER);
	if (length === okm.length) return okm;
	const res = okm.slice(0, length);
	clean$1(okm);
	return res;
}
/**
* HKDF (RFC 5869): derive keys from an initial input.
* Combines hkdf_extract + hkdf_expand in one step
* @param hash - hash function that would be used (e.g. sha256)
* @param ikm - input keying material, the initial key
* @param salt - optional salt value (a non-secret random value)
* @param info - optional context and application specific information bytes
* @param length - length of output keying material in bytes.
*   RFC 5869 §2.3 allows `0..255*HashLen`, so `0` returns an empty OKM.
* @returns Output keying material derived from the input key.
* @throws If the requested output length exceeds the HKDF limit
*   for the selected hash. {@link Error}
* @example
* HKDF (RFC 5869): derive keys from an initial input.
* ```ts
* import { hkdf } from '@noble/hashes/hkdf.js';
* import { sha256 } from '@noble/hashes/sha2.js';
* import { randomBytes, utf8ToBytes } from '@noble/hashes/utils.js';
* const inputKey = randomBytes(32);
* const salt = randomBytes(32);
* const info = utf8ToBytes('application-key');
* const okm = hkdf(sha256, inputKey, salt, info, 32);
* ```
*/
const hkdf = (hash, ikm, salt, info, length) => {
	ahash(hash);
	if (salt === void 0) salt = new Uint8Array(hash.outputLen);
	const HMAC = hmac.create(hash, salt).update(ikm);
	return expand(hash, HMAC.digest(), info, length, HMAC);
};
//#endregion
//#region msg-crypto.js
const MSG_VERSION = 1;
const MSG_TEXT_KEY = "msg";
const MAX_TEXT = 2e3;
const DEFAULT_PEER = "/dns4/west-territories-location-trek.trycloudflare.com/tcp/443/wss/p2p/16Uiu2HAmFQGCcQzKpY9QhESgVXWXUv4AHWWvR6pX6vE1qvFUpyoc";
const CLUSTER = {
	clusterId: 3,
	numShardsInCluster: 8
};
const INFO = new TextEncoder().encode("names/msg/v1");
const AUTH_INFO = new TextEncoder().encode("names/msg/v1/auth");
const HEADER_LEN = 57;
const NAME = /^[a-z0-9](?:[a-z0-9-]{0,30}[a-z0-9])?$/;
const utf8 = (text) => new TextEncoder().encode(text);
function toHex(bytes) {
	return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
function fromHex(hex) {
	if (typeof hex !== "string" || !/^(?:[0-9a-fA-F]{2})*$/.test(hex)) throw new Error("not hex");
	const out = new Uint8Array(hex.length / 2);
	for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(2 * i, 2 * i + 2), 16);
	return out;
}
function concat(...parts) {
	const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
	let at = 0;
	for (const p of parts) {
		out.set(p, at);
		at += p.length;
	}
	return out;
}
function equal(a, b) {
	if (a.length !== b.length) return false;
	let diff = 0;
	for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
	return diff === 0;
}
/** X25519 shared secret, refusing the all-zero result of a low-order key. */
function dh(secretKey, publicKey) {
	const shared = x25519.getSharedSecret(secretKey, publicKey);
	if (shared.every((b) => b === 0)) throw new Error("bad public key");
	return shared;
}
/** A new device messaging key pair: { secretKey, publicKey }, 32 bytes each. */
function newKeyPair() {
	const secretKey = x25519.utils.randomSecretKey();
	return {
		secretKey,
		publicKey: x25519.getPublicKey(secretKey)
	};
}
function publicKeyOf(secretKey) {
	return x25519.getPublicKey(secretKey);
}
/** The 32-byte key in a name's `msg` entry, or null when it isn't one. */
function parseMsgKey(value) {
	const hex = typeof value === "string" ? value.trim().toLowerCase() : "";
	return /^[0-9a-f]{64}$/.test(hex) ? fromHex(hex) : null;
}
/** The Waku content topic of the inbox of `publicKey`. */
function inboxTopic(publicKey) {
	return `/names/1/msg-${toHex(sha256(publicKey)).slice(0, 16)}/proto`;
}
function authTag(staticShared, header, from, fromKey, ts, text) {
	const key = hkdf(sha256, staticShared, /* @__PURE__ */ new Uint8Array(0), AUTH_INFO, 32);
	const data = concat(header, utf8(from), [0], fromKey, utf8(String(ts)), [0], utf8(text));
	return hmac(sha256, key, data);
}
/**
* Seals `text` from the name `from` (whose device key is `fromSecret`) to the
* device key `to`. Returns the wire payload.
*/
function sealMessage({ to, from, fromSecret, text, ts = Date.now() }) {
	if (!(to instanceof Uint8Array) || to.length !== 32) throw new Error("recipient key must be 32 bytes");
	if (!NAME.test(from)) throw new Error("from must be a name");
	if (typeof text !== "string" || text.length === 0 || text.length > 2e3) throw new Error("bad text");
	const fromKey = publicKeyOf(fromSecret);
	const eph = newKeyPair();
	const nonce = crypto.getRandomValues(/* @__PURE__ */ new Uint8Array(24));
	const header = concat([1], eph.publicKey, nonce);
	const key = hkdf(sha256, dh(eph.secretKey, to), concat(eph.publicKey, to), INFO, 32);
	const auth = toHex(authTag(dh(fromSecret, to), header, from, fromKey, ts, text));
	const plain = utf8(JSON.stringify({
		v: 1,
		from,
		fromKey: toHex(fromKey),
		text,
		ts,
		auth
	}));
	return concat(header, xchacha20poly1305(key, nonce, concat([1], eph.publicKey)).encrypt(plain));
}
/**
* Opens a wire payload with this device's secret key. Returns null when it isn't
* for this key or isn't well formed, otherwise
*   { id, from, fromKey (hex), text, ts, authentic }
* where `authentic` says the sender holds the secret key of `fromKey`. Whether
* `fromKey` belongs to the name `from` is for the caller to check on chain.
* `id` (the ephemeral key, hex) tells repeats apart.
*/
function openMessage(secretKey, payload) {
	try {
		if (!(payload instanceof Uint8Array) || payload.length < 73 || payload[0] !== 1) return null;
		const header = payload.subarray(0, HEADER_LEN);
		const ephPub = payload.subarray(1, 33);
		const nonce = payload.subarray(33, HEADER_LEN);
		const me = publicKeyOf(secretKey);
		const key = hkdf(sha256, dh(secretKey, ephPub), concat(ephPub, me), INFO, 32);
		const plain = xchacha20poly1305(key, nonce, concat([1], ephPub)).decrypt(payload.subarray(HEADER_LEN));
		const { v, from, fromKey, text, ts, auth } = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(plain)) ?? {};
		if (v !== 1 || typeof from !== "string" || !NAME.test(from)) return null;
		if (typeof text !== "string" || text.length > 2e3 || !Number.isSafeInteger(ts)) return null;
		const senderKey = parseMsgKey(fromKey);
		if (senderKey === null) return null;
		let authentic = false;
		try {
			const expected = authTag(dh(secretKey, senderKey), header, from, senderKey, ts, text);
			authentic = typeof auth === "string" && /^[0-9a-f]{64}$/.test(auth) && equal(expected, fromHex(auth));
		} catch {
			authentic = false;
		}
		return {
			id: toHex(ephPub),
			from,
			fromKey: toHex(senderKey),
			text,
			ts,
			authentic
		};
	} catch {
		return null;
	}
}
/**
* Whether a message opened by openMessage really comes from the name it claims:
* authentic, and `onChainKey` (the `msg` entry of `from`, as read from the chain)
* is its `fromKey`.
*/
function isVerified(message, onChainKey) {
	const key = parseMsgKey(onChainKey ?? "");
	return Boolean(message?.authentic && key && toHex(key) === message.fromKey);
}
//#endregion
export { CLUSTER, DEFAULT_PEER, MAX_TEXT, MSG_TEXT_KEY, MSG_VERSION, fromHex, inboxTopic, isVerified, newKeyPair, openMessage, parseMsgKey, publicKeyOf, sealMessage, toHex };
