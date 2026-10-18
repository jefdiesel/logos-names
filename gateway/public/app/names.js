// Generated from resolver/names.js with @noble/hashes bundled in. Do not edit; regenerate with:
//   cd gateway && npx rolldown ../resolver/names.js --format esm --file public/app/names.js --banner "..."
//#region ../node_modules/@noble/hashes/utils.js
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
function isBytes(a) {
	return a instanceof Uint8Array || ArrayBuffer.isView(a) && a.constructor.name === "Uint8Array" && "BYTES_PER_ELEMENT" in a && a.BYTES_PER_ELEMENT === 1;
}
const atitle = (title) => title ? `"${title}" ` : "";
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
function anumber(n, title = "") {
	if (typeof n !== "number") throw new TypeError(atitle(title) + "expected number, got " + typeof n);
	if (!Number.isSafeInteger(n) || n < 0) throw new RangeError(atitle(title) + "expected integer >= 0, got " + n);
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
/**
* Copies bytes into a fresh Uint8Array.
* Buffer-style slices can alias the same backing store, so callers that need ownership should copy.
* @param bytes - source bytes to clone
* @returns Freshly allocated copy of `bytes`.
* @throws On wrong argument types. {@link TypeError}
* @example
* Clone a byte array before mutating it.
* ```ts
* const copy = copyBytes(new Uint8Array([1, 2, 3]));
* ```
*/
function copyBytes(bytes) {
	return Uint8Array.from(abytes(bytes));
}
const aobject = (value, label) => {
	if (value === null || typeof value !== "object" || Array.isArray(value)) throw new TypeError((label === "object" ? "" : `"${label}" `) + "expected object, got type=" + typeof value);
};
const aopts = (value, label) => {
	aobject(value, label);
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
function aexists(instance, checkFinished = true) {
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
function aoutput(out, instance) {
	abytes(out, void 0, "output");
	const min = instance.outputLen;
	if (!(out.length >= min)) throw new RangeError("\"output\" expected length >= " + min);
}
/**
* Casts a typed array view to Uint8Array.
* @param arr - source typed array
* @returns Uint8Array view over the same buffer.
* @example
* Reinterpret a typed array as bytes.
* ```ts
* u8(new Uint32Array([1, 2]));
* ```
*/
function u8(arr) {
	return new Uint8Array(arr.buffer, arr.byteOffset, arr.byteLength);
}
/**
* Casts a typed array view to Uint32Array.
* `arr.byteOffset` must already be 4-byte aligned or the platform
* Uint32Array constructor will throw.
* @param arr - source typed array
* @returns Uint32Array view over the same buffer.
* @example
* Reinterpret a byte array as 32-bit words.
* ```ts
* u32(new Uint8Array(8));
* ```
*/
function u32(arr) {
	return new Uint32Array(arr.buffer, arr.byteOffset, Math.floor(arr.byteLength / 4));
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
function clean(...arrays) {
	for (let i = 0; i < arrays.length; i++) arrays[i].fill(0);
}
/** Whether the current platform is little-endian. */
const isLE = /* @__PURE__ */ (() => new Uint8Array(new Uint32Array([287454020]).buffer)[0] === 68)();
/**
* Byte-swap operation for uint32 values.
* @param word - source word
* @returns Word with reversed byte order.
* @example
* Reverse the byte order of a 32-bit word.
* ```ts
* byteSwap(0x11223344);
* ```
*/
function byteSwap(word) {
	return word << 24 & 4278190080 | word << 8 & 16711680 | word >>> 8 & 65280 | word >>> 24 & 255;
}
/**
* Conditionally byte-swaps one 32-bit word on big-endian platforms.
* @param n - source word
* @returns Original or byte-swapped word depending on platform endianness.
* @example
* Normalize a 32-bit word for host endianness.
* ```ts
* swap8IfBE(0x11223344);
* ```
*/
const swap8IfBE = isLE ? (n) => n : (n) => byteSwap(n) >>> 0;
/**
* Byte-swaps every word of a Uint32Array in place.
* @param arr - array to mutate
* @returns The same array after mutation; callers pass live state arrays here.
* @example
* Reverse the byte order of every word in place.
* ```ts
* byteSwap32(new Uint32Array([0x11223344]));
* ```
*/
function byteSwap32(arr) {
	for (let i = 0; i < arr.length; i++) arr[i] = byteSwap(arr[i]);
	return arr;
}
/**
* Conditionally byte-swaps a Uint32Array on big-endian platforms.
* @param u - array to normalize for host endianness
* @returns Original or byte-swapped array depending on platform endianness.
*   On big-endian runtimes this mutates `u` in place via `byteSwap32(...)`.
* @example
* Normalize a word array for host endianness.
* ```ts
* swap32IfBE(new Uint32Array([0x11223344]));
* ```
*/
const swap32IfBE = isLE ? (u) => u : byteSwap32;
/**
* Yields to the host task scheduler so timers, I/O, and rendering can make progress.
* Uses the Web Scheduling API when available or `setTimeout` as a cross-platform fallback.
* Host-task yields are much slower than microtasks (roughly 1ms with the timer fallback), so
* async loops should use `asyncTick >= 10` to amortize scheduling overhead to about 10% while
* still allowing other event-loop work to proceed.
* @param onReject - optional cleanup invoked only if the host yield fails
* @example
* Yield to the next scheduler tick.
* ```ts
* await nextTick();
* ```
*/
function nextTick(onReject) {
	const host = globalThis;
	if (typeof host.scheduler?.yield === "function") {
		const promise = host.scheduler.yield();
		if (onReject) promise.catch(onReject);
		return promise;
	}
	return new Promise((resolve) => host.setTimeout(resolve, 0));
}
/**
* Converts string to bytes using UTF8 encoding.
* Built-in doesn't validate input to be string: we do the check.
* Non-ASCII details are delegated to the platform `TextEncoder`.
* @param str - string to encode
* @returns UTF-8 encoded bytes.
* @throws On wrong argument types. {@link TypeError}
* @example
* Encode a string as UTF-8 bytes.
* ```ts
* utf8ToBytes('abc'); // Uint8Array.from([97, 98, 99])
* ```
*/
function utf8ToBytes(str) {
	if (typeof str !== "string") throw new TypeError("string expected");
	const encoded = new TextEncoder().encode(str);
	try {
		return new Uint8Array(encoded);
	} finally {
		clean(encoded);
	}
}
/**
* Helper for KDFs: consumes Uint8Array or string.
* String inputs are UTF-8 encoded; byte-array inputs stay aliased to the caller buffer.
* @param data - user-provided KDF input
* @param errorTitle - label included in thrown errors
* @returns Byte representation of the input.
* @throws On wrong argument types. {@link TypeError}
* @example
* Normalize KDF input to bytes.
* ```ts
* kdfInputToBytes('password');
* ```
*/
function kdfInputToBytes(data, errorTitle = "") {
	if (typeof data === "string") return utf8ToBytes(data);
	return abytes(data, void 0, errorTitle);
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
function checkOpts(defaults, opts, title = "opts") {
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
	info = checkOpts({}, info, "info");
	const hashC = (msg, opts) => hashCons(opts).update(msg).digest();
	const tmp = hashCons(void 0);
	hashC.outputLen = tmp.outputLen;
	hashC.blockLen = tmp.blockLen;
	hashC.canXOF = tmp.canXOF;
	hashC.create = (opts) => hashCons(opts);
	Object.assign(hashC, info);
	return Object.freeze(hashC);
}
//#endregion
//#region ../node_modules/@noble/hashes/_blake.js
/**
* Internal blake permutation table.
* Rows `0..9` serve BLAKE2s, rows `0..11` serve BLAKE2b with `10..11 = 0..1`, and Blake1 also
* reuses the later rows shown below. Blake1 expands rounds `10..15` as `SIGMA[i % 10]`, so rows
* `10..15` intentionally repeat rows `0..5` for the 14-round (256) and 16-round (512) variants.
*/
const BSIGMA = /* @__PURE__ */ Uint8Array.from([
	0,
	1,
	2,
	3,
	4,
	5,
	6,
	7,
	8,
	9,
	10,
	11,
	12,
	13,
	14,
	15,
	14,
	10,
	4,
	8,
	9,
	15,
	13,
	6,
	1,
	12,
	0,
	2,
	11,
	7,
	5,
	3,
	11,
	8,
	12,
	0,
	5,
	2,
	15,
	13,
	10,
	14,
	3,
	6,
	7,
	1,
	9,
	4,
	7,
	9,
	3,
	1,
	13,
	12,
	11,
	14,
	2,
	6,
	5,
	10,
	4,
	0,
	15,
	8,
	9,
	0,
	5,
	7,
	2,
	4,
	10,
	15,
	14,
	1,
	11,
	12,
	6,
	8,
	3,
	13,
	2,
	12,
	6,
	10,
	0,
	11,
	8,
	3,
	4,
	13,
	7,
	5,
	15,
	14,
	1,
	9,
	12,
	5,
	1,
	15,
	14,
	13,
	4,
	10,
	0,
	7,
	6,
	3,
	9,
	2,
	8,
	11,
	13,
	11,
	7,
	14,
	12,
	1,
	3,
	9,
	5,
	0,
	15,
	4,
	8,
	6,
	2,
	10,
	6,
	15,
	14,
	9,
	11,
	3,
	0,
	8,
	12,
	2,
	13,
	7,
	1,
	4,
	10,
	5,
	10,
	2,
	8,
	4,
	7,
	6,
	1,
	5,
	15,
	11,
	9,
	14,
	3,
	12,
	13,
	0,
	0,
	1,
	2,
	3,
	4,
	5,
	6,
	7,
	8,
	9,
	10,
	11,
	12,
	13,
	14,
	15,
	14,
	10,
	4,
	8,
	9,
	15,
	13,
	6,
	1,
	12,
	0,
	2,
	11,
	7,
	5,
	3,
	11,
	8,
	12,
	0,
	5,
	2,
	15,
	13,
	10,
	14,
	3,
	6,
	7,
	1,
	9,
	4,
	7,
	9,
	3,
	1,
	13,
	12,
	11,
	14,
	2,
	6,
	5,
	10,
	4,
	0,
	15,
	8,
	9,
	0,
	5,
	7,
	2,
	4,
	10,
	15,
	14,
	1,
	11,
	12,
	6,
	8,
	3,
	13,
	2,
	12,
	6,
	10,
	0,
	11,
	8,
	3,
	4,
	13,
	7,
	5,
	15,
	14,
	1,
	9
]);
//#endregion
//#region ../node_modules/@noble/hashes/_u64.js
const fromNumH = (n) => n / 2 ** 32 | 0;
const fromNumL = (n) => n >>> 0;
const rotrSH = (h, l, s) => h >>> s | l << 32 - s;
const rotrSL = (h, l, s) => h << 32 - s | l >>> s;
const rotrBH = (h, l, s) => h << 64 - s | l >>> s - 32;
const rotrBL = (h, l, s) => h >>> s - 32 | l << 64 - s;
const rotr32H = (_h, l) => l;
const rotr32L = (h, _l) => h;
function add(Ah, Al, Bh, Bl) {
	const l = (Al >>> 0) + (Bl >>> 0);
	return {
		h: Ah + Bh + (l / 2 ** 32 | 0) | 0,
		l: l | 0
	};
}
const add3L = (Al, Bl, Cl) => (Al >>> 0) + (Bl >>> 0) + (Cl >>> 0);
const add3H = (low, Ah, Bh, Ch) => Ah + Bh + Ch + (low / 2 ** 32 | 0) | 0;
//#endregion
//#region ../node_modules/@noble/hashes/blake2.js
/**
* blake2b (64-bit) & blake2s (8 to 32-bit) hash functions.
* b could have been faster, but there is no fast u64 in js, so s is 1.5x faster.
* @module
*/
const B2B_IV = /* @__PURE__ */ Uint32Array.from([
	4089235720,
	1779033703,
	2227873595,
	3144134277,
	4271175723,
	1013904242,
	1595750129,
	2773480762,
	2917565137,
	1359893119,
	725511199,
	2600822924,
	4215389547,
	528734635,
	327033209,
	1541459225
]);
const BBUF = /* @__PURE__ */ new Uint32Array(32);
function G1b(a, b, c, d, msg, x) {
	const Xl = msg[x], Xh = msg[x + 1];
	let Al = BBUF[2 * a], Ah = BBUF[2 * a + 1];
	let Bl = BBUF[2 * b], Bh = BBUF[2 * b + 1];
	let Cl = BBUF[2 * c], Ch = BBUF[2 * c + 1];
	let Dl = BBUF[2 * d], Dh = BBUF[2 * d + 1];
	const ll = add3L(Al, Bl, Xl);
	Ah = add3H(ll, Ah, Bh, Xh);
	Al = ll | 0;
	let xh = Dh ^ Ah, xl = Dl ^ Al;
	Dh = rotr32H(xh, xl);
	Dl = rotr32L(xh, xl);
	({h: Ch, l: Cl} = add(Ch, Cl, Dh, Dl));
	xh = Bh ^ Ch;
	xl = Bl ^ Cl;
	Bh = rotrSH(xh, xl, 24);
	Bl = rotrSL(xh, xl, 24);
	BBUF[2 * a] = Al;
	BBUF[2 * a + 1] = Ah;
	BBUF[2 * b] = Bl;
	BBUF[2 * b + 1] = Bh;
	BBUF[2 * c] = Cl;
	BBUF[2 * c + 1] = Ch;
	BBUF[2 * d] = Dl;
	BBUF[2 * d + 1] = Dh;
}
function G2b(a, b, c, d, msg, x) {
	const Xl = msg[x], Xh = msg[x + 1];
	let Al = BBUF[2 * a], Ah = BBUF[2 * a + 1];
	let Bl = BBUF[2 * b], Bh = BBUF[2 * b + 1];
	let Cl = BBUF[2 * c], Ch = BBUF[2 * c + 1];
	let Dl = BBUF[2 * d], Dh = BBUF[2 * d + 1];
	const ll = add3L(Al, Bl, Xl);
	Ah = add3H(ll, Ah, Bh, Xh);
	Al = ll | 0;
	let xh = Dh ^ Ah, xl = Dl ^ Al;
	Dh = rotrSH(xh, xl, 16);
	Dl = rotrSL(xh, xl, 16);
	({h: Ch, l: Cl} = add(Ch, Cl, Dh, Dl));
	xh = Bh ^ Ch;
	xl = Bl ^ Cl;
	Bh = rotrBH(xh, xl, 63);
	Bl = rotrBL(xh, xl, 63);
	BBUF[2 * a] = Al;
	BBUF[2 * a + 1] = Ah;
	BBUF[2 * b] = Bl;
	BBUF[2 * b + 1] = Bh;
	BBUF[2 * c] = Cl;
	BBUF[2 * c + 1] = Ch;
	BBUF[2 * d] = Dl;
	BBUF[2 * d + 1] = Dh;
}
function checkBlake2Opts(outputLen, opts = {}, keyLen, saltLen, persLen) {
	anumber(keyLen);
	if (outputLen <= 0 || outputLen > keyLen) throw new Error("\"dkLen\" must be 1.." + keyLen + ", got " + outputLen);
	const { key, salt, personalization } = opts;
	if (key !== void 0 && (key.length < 1 || key.length > keyLen)) throw new Error("\"key\" expected to be undefined or of length=1.." + keyLen);
	if (salt !== void 0) abytes(salt, saltLen, "salt");
	if (personalization !== void 0) abytes(personalization, persLen, "personalization");
}
/** Internal base class for BLAKE2. */
var _BLAKE2 = class {
	buffer;
	buffer32;
	finished = false;
	destroyed = false;
	length = 0;
	pos = 0;
	blockLen;
	outputLen;
	canXOF = false;
	constructor(blockLen, outputLen) {
		anumber(blockLen);
		anumber(outputLen);
		this.blockLen = blockLen;
		this.outputLen = outputLen;
		this.buffer = new Uint8Array(blockLen);
		this.buffer32 = u32(this.buffer);
	}
	update(data) {
		aexists(this);
		abytes(data);
		const { blockLen, buffer, buffer32 } = this;
		const len = data.length;
		const offset = data.byteOffset;
		const buf = data.buffer;
		for (let pos = 0; pos < len;) {
			if (this.pos === blockLen) {
				swap32IfBE(buffer32);
				this.compress(buffer32, 0, false);
				swap32IfBE(buffer32);
				this.pos = 0;
			}
			const take = Math.min(blockLen - this.pos, len - pos);
			const dataOffset = offset + pos;
			if (take === blockLen && !(dataOffset % 4) && pos + take < len) {
				const data32 = new Uint32Array(buf, dataOffset, Math.floor((len - pos) / 4));
				swap32IfBE(data32);
				for (let pos32 = 0; pos + blockLen < len; pos32 += buffer32.length, pos += blockLen) {
					this.length += blockLen;
					this.compress(data32, pos32, false);
				}
				swap32IfBE(data32);
				continue;
			}
			buffer.set(pos === 0 && take === len ? data : data.subarray(pos, pos + take), this.pos);
			this.pos += take;
			this.length += take;
			pos += take;
		}
		return this;
	}
	digestInto(out) {
		aexists(this);
		aoutput(out, this);
		if (out.byteOffset & 3) throw new RangeError("\"output\" expected 4-byte aligned byteOffset, got " + out.byteOffset);
		const { pos, buffer32 } = this;
		this.finished = true;
		this.buffer.fill(0, pos);
		swap32IfBE(buffer32);
		this.compress(buffer32, 0, true);
		swap32IfBE(buffer32);
		const state = this.get();
		const out32 = out === this.buffer ? buffer32 : u32(out);
		const full = Math.floor(this.outputLen / 4);
		for (let i = 0; i < full; i++) out32[i] = swap8IfBE(state[i]);
		const tail = this.outputLen % 4;
		if (!tail) return;
		const off = full * 4;
		const word = state[full];
		for (let i = 0; i < tail; i++) out[off + i] = word >>> 8 * i;
	}
	digest() {
		const { buffer, outputLen } = this;
		this.digestInto(buffer);
		const res = buffer.slice(0, outputLen);
		this.destroy();
		return res;
	}
	_cloneInto(to) {
		const { buffer, length, finished, destroyed, outputLen, pos } = this;
		to ||= new this.constructor({ dkLen: outputLen });
		to.set(...this.get());
		to.buffer.set(buffer);
		to.destroyed = destroyed;
		to.finished = finished;
		to.length = length;
		to.pos = pos;
		to.outputLen = outputLen;
		return to;
	}
	clone() {
		return this._cloneInto();
	}
};
/** Internal blake2b hash class with state stored as LE u32 low/high halves. */
var _BLAKE2b = class extends _BLAKE2 {
	v0l = B2B_IV[0] | 0;
	v0h = B2B_IV[1] | 0;
	v1l = B2B_IV[2] | 0;
	v1h = B2B_IV[3] | 0;
	v2l = B2B_IV[4] | 0;
	v2h = B2B_IV[5] | 0;
	v3l = B2B_IV[6] | 0;
	v3h = B2B_IV[7] | 0;
	v4l = B2B_IV[8] | 0;
	v4h = B2B_IV[9] | 0;
	v5l = B2B_IV[10] | 0;
	v5h = B2B_IV[11] | 0;
	v6l = B2B_IV[12] | 0;
	v6h = B2B_IV[13] | 0;
	v7l = B2B_IV[14] | 0;
	v7h = B2B_IV[15] | 0;
	constructor(opts = {}) {
		opts = checkOpts({}, opts);
		const olen = opts.dkLen === void 0 ? 64 : opts.dkLen;
		super(128, olen);
		checkBlake2Opts(olen, opts, 64, 16, 16);
		let { key, personalization, salt } = opts;
		let keyLength = 0;
		if (key !== void 0) {
			abytes(key, void 0, "key");
			keyLength = key.length;
		}
		this.v0l ^= this.outputLen | keyLength << 8 | 16842752;
		if (salt !== void 0) {
			abytes(salt, void 0, "salt");
			const slt = u32(copyBytes(salt));
			this.v4l ^= swap8IfBE(slt[0]);
			this.v4h ^= swap8IfBE(slt[1]);
			this.v5l ^= swap8IfBE(slt[2]);
			this.v5h ^= swap8IfBE(slt[3]);
		}
		if (personalization !== void 0) {
			abytes(personalization, void 0, "personalization");
			const pers = u32(copyBytes(personalization));
			this.v6l ^= swap8IfBE(pers[0]);
			this.v6h ^= swap8IfBE(pers[1]);
			this.v7l ^= swap8IfBE(pers[2]);
			this.v7h ^= swap8IfBE(pers[3]);
		}
		if (key !== void 0) {
			const tmp = new Uint8Array(this.blockLen);
			tmp.set(key);
			this.update(tmp);
			clean(tmp);
		}
	}
	get() {
		let { v0l, v0h, v1l, v1h, v2l, v2h, v3l, v3h, v4l, v4h, v5l, v5h, v6l, v6h, v7l, v7h } = this;
		return [
			v0l,
			v0h,
			v1l,
			v1h,
			v2l,
			v2h,
			v3l,
			v3h,
			v4l,
			v4h,
			v5l,
			v5h,
			v6l,
			v6h,
			v7l,
			v7h
		];
	}
	set(v0l, v0h, v1l, v1h, v2l, v2h, v3l, v3h, v4l, v4h, v5l, v5h, v6l, v6h, v7l, v7h) {
		this.v0l = v0l | 0;
		this.v0h = v0h | 0;
		this.v1l = v1l | 0;
		this.v1h = v1h | 0;
		this.v2l = v2l | 0;
		this.v2h = v2h | 0;
		this.v3l = v3l | 0;
		this.v3h = v3h | 0;
		this.v4l = v4l | 0;
		this.v4h = v4h | 0;
		this.v5l = v5l | 0;
		this.v5h = v5h | 0;
		this.v6l = v6l | 0;
		this.v6h = v6h | 0;
		this.v7l = v7l | 0;
		this.v7h = v7h | 0;
	}
	compress(msg, offset, isLast) {
		const { v0l, v0h, v1l, v1h, v2l, v2h, v3l, v3h, v4l, v4h, v5l, v5h, v6l, v6h, v7l, v7h } = this;
		BBUF[0] = v0l;
		BBUF[1] = v0h;
		BBUF[2] = v1l;
		BBUF[3] = v1h;
		BBUF[4] = v2l;
		BBUF[5] = v2h;
		BBUF[6] = v3l;
		BBUF[7] = v3h;
		BBUF[8] = v4l;
		BBUF[9] = v4h;
		BBUF[10] = v5l;
		BBUF[11] = v5h;
		BBUF[12] = v6l;
		BBUF[13] = v6h;
		BBUF[14] = v7l;
		BBUF[15] = v7h;
		BBUF.set(B2B_IV, 16);
		const l = fromNumL(this.length);
		const h = fromNumH(this.length);
		BBUF[24] = B2B_IV[8] ^ l;
		BBUF[25] = B2B_IV[9] ^ h;
		if (isLast) {
			BBUF[28] = ~BBUF[28];
			BBUF[29] = ~BBUF[29];
		}
		let j = 0;
		const s = BSIGMA;
		for (let i = 0; i < 12; i++) {
			G1b(0, 4, 8, 12, msg, offset + 2 * s[j++]);
			G2b(0, 4, 8, 12, msg, offset + 2 * s[j++]);
			G1b(1, 5, 9, 13, msg, offset + 2 * s[j++]);
			G2b(1, 5, 9, 13, msg, offset + 2 * s[j++]);
			G1b(2, 6, 10, 14, msg, offset + 2 * s[j++]);
			G2b(2, 6, 10, 14, msg, offset + 2 * s[j++]);
			G1b(3, 7, 11, 15, msg, offset + 2 * s[j++]);
			G2b(3, 7, 11, 15, msg, offset + 2 * s[j++]);
			G1b(0, 5, 10, 15, msg, offset + 2 * s[j++]);
			G2b(0, 5, 10, 15, msg, offset + 2 * s[j++]);
			G1b(1, 6, 11, 12, msg, offset + 2 * s[j++]);
			G2b(1, 6, 11, 12, msg, offset + 2 * s[j++]);
			G1b(2, 7, 8, 13, msg, offset + 2 * s[j++]);
			G2b(2, 7, 8, 13, msg, offset + 2 * s[j++]);
			G1b(3, 4, 9, 14, msg, offset + 2 * s[j++]);
			G2b(3, 4, 9, 14, msg, offset + 2 * s[j++]);
		}
		this.v0l ^= BBUF[0] ^ BBUF[16];
		this.v0h ^= BBUF[1] ^ BBUF[17];
		this.v1l ^= BBUF[2] ^ BBUF[18];
		this.v1h ^= BBUF[3] ^ BBUF[19];
		this.v2l ^= BBUF[4] ^ BBUF[20];
		this.v2h ^= BBUF[5] ^ BBUF[21];
		this.v3l ^= BBUF[6] ^ BBUF[22];
		this.v3h ^= BBUF[7] ^ BBUF[23];
		this.v4l ^= BBUF[8] ^ BBUF[24];
		this.v4h ^= BBUF[9] ^ BBUF[25];
		this.v5l ^= BBUF[10] ^ BBUF[26];
		this.v5h ^= BBUF[11] ^ BBUF[27];
		this.v6l ^= BBUF[12] ^ BBUF[28];
		this.v6h ^= BBUF[13] ^ BBUF[29];
		this.v7l ^= BBUF[14] ^ BBUF[30];
		this.v7h ^= BBUF[15] ^ BBUF[31];
		clean(BBUF);
	}
	destroy() {
		this.destroyed = true;
		clean(this.buffer32);
		this.set(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
	}
};
/**
* Blake2b hash function. 64-bit. 1.5x slower than blake2s in JS.
* @param msg - message that would be hashed
* @param opts - Optional output, MAC, salt, and personalization settings.
*   `dkLen` must be 1..64 bytes; `salt` and `personalization`, if present,
*   must be 16 bytes each. See {@link Blake2Opts}.
* @returns Digest bytes.
* @example
* Hash a message with Blake2b.
* ```ts
* blake2b(new Uint8Array([97, 98, 99]));
* ```
* @example
* Hash a message with Blake2b while selecting output, MAC, salt, and personalization settings.
* ```ts
* blake2b(new Uint8Array([97, 98, 99]), {
*   dkLen: 32,
*   key: new Uint8Array(32),
*   salt: new Uint8Array(16),
*   personalization: new Uint8Array(16),
* });
* ```
*/
const blake2b = /* @__PURE__ */ createHasher((opts) => new _BLAKE2b(opts));
//#endregion
//#region ../node_modules/@noble/hashes/argon2.js
/**
* Argon2 KDF from RFC 9106. Can be used to create a key from password and salt.
* We suggest to use Scrypt. JS Argon is 2-10x slower than native code because of 64-bitness:
* * argon uses uint64, but JS doesn't have fast uint64array
* * uint64 multiplication is 1/3 of time
* * `P` function would be very nice with u64, because most of value will be in registers,
*   hovewer with u32 it will require 32 registers, which is too much.
* * JS arrays do slow bound checks, so reading from `A2_BUF` slows it down
* @module
*/
const AT = {
	Argon2d: 0,
	Argon2i: 1,
	Argon2id: 2
};
const ARGON2_SYNC_POINTS = 4;
const abytesOrZero = (buf, errorTitle = "") => {
	if (buf === void 0) return Uint8Array.of();
	return kdfInputToBytes(buf, errorTitle);
};
const A2_BUF = /* @__PURE__ */ new Uint32Array(256);
function G(a, b, c, d) {
	let Al = A2_BUF[2 * a], Ah = A2_BUF[2 * a + 1];
	let Bl = A2_BUF[2 * b], Bh = A2_BUF[2 * b + 1];
	let Cl = A2_BUF[2 * c], Ch = A2_BUF[2 * c + 1];
	let Dl = A2_BUF[2 * d], Dh = A2_BUF[2 * d + 1];
	let ml = 0, mh = 0, rl = 0, xh = 0, xl = 0;
	ml = Math.imul(Al, Bl);
	mh = ((Al >>> 0) * (Bl >>> 0) - (ml >>> 0)) / 4294967296 + .5 | 0;
	rl = (Al >>> 0) + (Bl >>> 0) + (ml << 1 >>> 0);
	Ah = Ah + Bh + (mh << 1 | ml >>> 31) + (rl / 4294967296 | 0) | 0;
	Al = rl | 0;
	xh = Dh ^ Ah;
	xl = Dl ^ Al;
	Dh = xl;
	Dl = xh;
	ml = Math.imul(Cl, Dl);
	mh = ((Cl >>> 0) * (Dl >>> 0) - (ml >>> 0)) / 4294967296 + .5 | 0;
	rl = (Cl >>> 0) + (Dl >>> 0) + (ml << 1 >>> 0);
	Ch = Ch + Dh + (mh << 1 | ml >>> 31) + (rl / 4294967296 | 0) | 0;
	Cl = rl | 0;
	xh = Bh ^ Ch;
	xl = Bl ^ Cl;
	Bh = xh >>> 24 | xl << 8;
	Bl = xh << 8 | xl >>> 24;
	ml = Math.imul(Al, Bl);
	mh = ((Al >>> 0) * (Bl >>> 0) - (ml >>> 0)) / 4294967296 + .5 | 0;
	rl = (Al >>> 0) + (Bl >>> 0) + (ml << 1 >>> 0);
	Ah = Ah + Bh + (mh << 1 | ml >>> 31) + (rl / 4294967296 | 0) | 0;
	Al = rl | 0;
	xh = Dh ^ Ah;
	xl = Dl ^ Al;
	Dh = xh >>> 16 | xl << 16;
	Dl = xh << 16 | xl >>> 16;
	ml = Math.imul(Cl, Dl);
	mh = ((Cl >>> 0) * (Dl >>> 0) - (ml >>> 0)) / 4294967296 + .5 | 0;
	rl = (Cl >>> 0) + (Dl >>> 0) + (ml << 1 >>> 0);
	Ch = Ch + Dh + (mh << 1 | ml >>> 31) + (rl / 4294967296 | 0) | 0;
	Cl = rl | 0;
	xh = Bh ^ Ch;
	xl = Bl ^ Cl;
	Bh = xh << 1 | xl >>> 31;
	Bl = xh >>> 31 | xl << 1;
	A2_BUF[2 * a] = Al, A2_BUF[2 * a + 1] = Ah;
	A2_BUF[2 * b] = Bl, A2_BUF[2 * b + 1] = Bh;
	A2_BUF[2 * c] = Cl, A2_BUF[2 * c + 1] = Ch;
	A2_BUF[2 * d] = Dl, A2_BUF[2 * d + 1] = Dh;
}
function P(v00, v01, v02, v03, v04, v05, v06, v07, v08, v09, v10, v11, v12, v13, v14, v15) {
	G(v00, v04, v08, v12);
	G(v01, v05, v09, v13);
	G(v02, v06, v10, v14);
	G(v03, v07, v11, v15);
	G(v00, v05, v10, v15);
	G(v01, v06, v11, v12);
	G(v02, v07, v08, v13);
	G(v03, v04, v09, v14);
}
function block(x, xPos, yPos, outPos, needXor) {
	if (needXor) for (let i = 0; i < 256; i++) {
		const r = x[xPos + i] ^ x[yPos + i];
		A2_BUF[i] = r;
		x[outPos + i] ^= r;
	}
	else for (let i = 0; i < 256; i++) {
		const r = x[xPos + i] ^ x[yPos + i];
		A2_BUF[i] = r;
		x[outPos + i] = r;
	}
	for (let i = 0; i < 128; i += 16) P(i, i + 1, i + 2, i + 3, i + 4, i + 5, i + 6, i + 7, i + 8, i + 9, i + 10, i + 11, i + 12, i + 13, i + 14, i + 15);
	for (let i = 0; i < 16; i += 2) P(i, i + 1, i + 16, i + 17, i + 32, i + 33, i + 48, i + 49, i + 64, i + 65, i + 80, i + 81, i + 96, i + 97, i + 112, i + 113);
	for (let i = 0; i < 256; i++) x[outPos + i] ^= A2_BUF[i];
	clean(A2_BUF);
}
function Hp(A, dkLen) {
	const A8 = u8(A);
	const T = /* @__PURE__ */ new Uint32Array(1);
	const T8 = u8(T);
	T[0] = swap8IfBE(dkLen);
	if (dkLen <= 64) return blake2b.create({ dkLen }).update(T8).update(A8).digest();
	const out = new Uint8Array(dkLen);
	let V = blake2b.create({}).update(T8).update(A8).digest();
	let pos = 0;
	out.set(V.subarray(0, 32));
	pos += 32;
	for (; dkLen - pos > 64; pos += 32) {
		const Vh = blake2b.create({}).update(V);
		Vh.digestInto(V);
		Vh.destroy();
		out.set(V.subarray(0, 32), pos);
	}
	out.set(blake2b(V, { dkLen: dkLen - pos }), pos);
	clean(V, T);
	return out;
}
function indexAlpha(r, s, laneLen, segmentLen, index, randL, sameLane = false) {
	let area;
	if (r === 0) {
		if (s === 0) area = index - 1;
		else if (sameLane) area = s * segmentLen + index - 1;
		else area = s * segmentLen + (index == 0 ? -1 : 0);
	} else if (sameLane) area = laneLen - segmentLen + index - 1;
	else area = laneLen - segmentLen + (index == 0 ? -1 : 0);
	const startPos = r !== 0 && s !== 3 ? (s + 1) * segmentLen : 0;
	const randLow = Math.imul(randL, randL);
	const randHigh = ((randL >>> 0) * (randL >>> 0) - (randLow >>> 0)) / 4294967296 + .5 | 0;
	const areaLow = Math.imul(area, randHigh);
	const areaHigh = ((area >>> 0) * (randHigh >>> 0) - (areaLow >>> 0)) / 4294967296 + .5 | 0;
	return (startPos + (area - 1 - areaHigh)) % laneLen;
}
const maxUint32 = Math.pow(2, 32);
const ARGON2_DEFAULT_MEMORY = 1024 ** 2;
const ARGON2_DEFAULT_MAXMEM = ARGON2_DEFAULT_MEMORY * 1024;
function isU32(num) {
	return Number.isSafeInteger(num) && num >= 0 && num < maxUint32;
}
function argon2Opts(opts = {}) {
	opts = checkOpts({}, opts);
	const merged = {
		t: 3,
		m: ARGON2_DEFAULT_MEMORY,
		p: 1,
		version: 19,
		dkLen: 32,
		maxmem: ARGON2_DEFAULT_MAXMEM,
		asyncTick: 10
	};
	for (let [k, v] of Object.entries(opts)) if (v !== void 0) merged[k] = v;
	const { dkLen, p, m, t, version, onProgress, asyncTick } = merged;
	if (!isU32(dkLen) || dkLen < 4) throw new Error("\"dkLen\" must be 4..");
	if (!isU32(p) || p < 1 || p >= Math.pow(2, 24)) throw new Error("\"p\" must be 1..2^24");
	if (!isU32(m)) throw new Error("\"m\" must be 0..2^32");
	if (!isU32(t) || t < 1) throw new Error("\"t\" (iterations) must be 1..2^32");
	if (onProgress !== void 0 && typeof onProgress !== "function") throw new Error("\"onProgress\" must be a function");
	anumber(asyncTick, "asyncTick");
	if (!isU32(m) || m < 8 * p) throw new Error("\"m\" (memory) must be at least 8*p bytes");
	if (version !== 16 && version !== 19) throw new Error("\"version\" must be 0x10 or 0x13, got " + version);
	return merged;
}
function argon2InitialHash(password, salt, type, opts) {
	const ownedInputs = [];
	const BUF = /* @__PURE__ */ new Uint32Array(1);
	const BUF8 = u8(BUF);
	let h;
	let H0;
	let succeeded = false;
	const rememberOwned = (input, bytes) => {
		if (typeof input === "string") ownedInputs.push(bytes);
		return bytes;
	};
	try {
		const passwordBytes = rememberOwned(password, kdfInputToBytes(password, "password"));
		const saltBytes = rememberOwned(salt, kdfInputToBytes(salt, "salt"));
		if (!isU32(passwordBytes.length)) throw new Error("\"password\" must be less of length 1..4Gb");
		if (!isU32(saltBytes.length) || saltBytes.length < 8) throw new Error("\"salt\" must be of length 8..4Gb");
		if (!Object.values(AT).includes(type)) throw new Error("\"type\" was invalid");
		let { p, dkLen, m, t, version, key, personalization, maxmem, onProgress, asyncTick } = argon2Opts(opts);
		const keyInput = key;
		key = rememberOwned(keyInput, abytesOrZero(keyInput, "key"));
		const personalizationInput = personalization;
		personalization = rememberOwned(personalizationInput, abytesOrZero(personalizationInput, "personalization"));
		h = blake2b.create();
		for (let item of [
			p,
			dkLen,
			m,
			t,
			version,
			type
		]) {
			BUF[0] = swap8IfBE(item);
			h.update(BUF8);
		}
		for (let i of [
			passwordBytes,
			saltBytes,
			key,
			personalization
		]) {
			BUF[0] = swap8IfBE(i.length);
			h.update(BUF8).update(i);
		}
		H0 = /* @__PURE__ */ new Uint32Array(18);
		h.digestInto(u8(H0));
		succeeded = true;
		return {
			H0,
			p,
			dkLen,
			m,
			t,
			version,
			maxmem,
			onProgress,
			asyncTick
		};
	} finally {
		if (h) h.destroy();
		clean(BUF, ...ownedInputs);
		if (!succeeded && H0) clean(H0);
	}
}
function argon2Init(password, salt, type, opts) {
	const { H0, p, dkLen, m, t, version, maxmem, onProgress, asyncTick } = argon2InitialHash(password, salt, type, opts);
	try {
		const lanes = p;
		const mP = 4 * p * Math.floor(m / (ARGON2_SYNC_POINTS * p));
		const laneLen = Math.floor(mP / p);
		const segmentLen = Math.floor(laneLen / ARGON2_SYNC_POINTS);
		const memUsed = mP * 1024;
		if (!isU32(maxmem)) throw new Error("\"maxmem\" expected <2**32, got " + maxmem);
		if (memUsed > maxmem) throw new Error("\"maxmem\" limit was hit: memUsed(mP*1024)=" + memUsed + ", maxmem=" + maxmem);
		const B = new Uint32Array(memUsed / 4);
		for (let l = 0; l < p; l++) {
			const i = 256 * laneLen * l;
			H0[17] = swap8IfBE(l);
			H0[16] = swap8IfBE(0);
			B.set(swap32IfBE(u32(Hp(H0, 1024))), i);
			H0[16] = swap8IfBE(1);
			B.set(swap32IfBE(u32(Hp(H0, 1024))), i + 256);
		}
		let perBlock = () => {};
		if (onProgress) {
			const totalBlock = t * ARGON2_SYNC_POINTS * p * segmentLen - 2 * p;
			const callbackPer = Math.max(Math.floor(totalBlock / 1e4), 1);
			let blockCnt = 0;
			perBlock = () => {
				blockCnt++;
				if (onProgress && (!(blockCnt % callbackPer) || blockCnt === totalBlock)) onProgress(blockCnt / totalBlock);
			};
		}
		return {
			type,
			mP,
			p,
			t,
			version,
			B,
			laneLen,
			lanes,
			segmentLen,
			dkLen,
			perBlock,
			asyncTick
		};
	} finally {
		clean(H0);
	}
}
function argon2Output(B, p, laneLen, dkLen) {
	const B_final = /* @__PURE__ */ new Uint32Array(256);
	for (let l = 0; l < p; l++) for (let j = 0; j < 256; j++) B_final[j] ^= B[256 * (laneLen * l + laneLen - 1) + j];
	const res = Hp(swap32IfBE(B_final), dkLen);
	clean(B, B_final);
	return res;
}
/**
* Fills every Argon2 block for all passes / slices / lanes, yielding once per
* processed block so callers control pacing: the sync driver just drains the
* generator, while the async driver awaits `nextTick()` between time slices.
*/
function* argon2Blocks(ctx, address) {
	const { type, mP, p, t, version, B, laneLen, lanes, segmentLen, perBlock } = ctx;
	address[262] = mP;
	address[264] = t;
	address[266] = type;
	for (let r = 0; r < t; r++) {
		const needXor = r !== 0 && version === 19;
		address[256] = r;
		for (let s = 0; s < ARGON2_SYNC_POINTS; s++) {
			address[260] = s;
			const dataIndependent = type == AT.Argon2i || type == AT.Argon2id && r === 0 && s < 2;
			for (let l = 0; l < p; l++) {
				address[258] = l;
				address[268] = 0;
				let startPos = 0;
				if (r === 0 && s === 0) {
					startPos = 2;
					if (dataIndependent) {
						address[268]++;
						block(address, 256, 512, 0, false);
						block(address, 0, 512, 0, false);
					}
				}
				let offset = l * laneLen + s * segmentLen + startPos;
				for (let index = startPos; index < segmentLen; index++, offset++) {
					perBlock();
					const prev = offset % laneLen ? offset - 1 : offset + laneLen - 1;
					let randL, randH;
					if (dataIndependent) {
						let i128 = index % 128;
						if (i128 === 0) {
							address[268]++;
							block(address, 256, 512, 0, false);
							block(address, 0, 512, 0, false);
						}
						randL = address[2 * i128];
						randH = address[2 * i128 + 1];
					} else {
						const T = 256 * prev;
						randL = B[T];
						randH = B[T + 1];
					}
					const refLane = r === 0 && s === 0 ? l : randH % lanes;
					const refPos = indexAlpha(r, s, laneLen, segmentLen, index, randL, refLane == l);
					const refBlock = laneLen * refLane + refPos;
					block(B, 256 * prev, 256 * refBlock, offset * 256, needXor);
					yield;
				}
			}
		}
	}
	clean(address);
}
async function argon2Async(type, password, salt, opts) {
	const ctx = argon2Init(password, salt, type, opts);
	const address = /* @__PURE__ */ new Uint32Array(768);
	const blocks = argon2Blocks(ctx, address);
	const abort = () => clean(address, ctx.B);
	let ts = Date.now();
	while (!blocks.next().done) {
		const diff = Date.now() - ts;
		if (diff >= 0 && diff < ctx.asyncTick) continue;
		await nextTick(abort);
		ts = Date.now();
	}
	return argon2Output(ctx.B, ctx.p, ctx.laneLen, ctx.dkLen);
}
/**
* Argon2id async, combining i+d, the most popular version from RFC 9106.
* @param password - password or input key material
* @param salt - unique salt value
* @param opts - Argon2 cost and optional tuning parameters. See {@link ArgonOpts}.
* @returns Promise resolving to derived key bytes.
* @throws If the Argon2 input or cost parameters are invalid. {@link Error}
* @example
* Derive a key with Argon2id asynchronously.
* ```ts
* await argon2idAsync('password', 'salt1234', { t: 1, m: 8, p: 1, dkLen: 32 });
* ```
*/
const argon2idAsync = (password, salt, opts = {}) => argon2Async(AT.Argon2id, password, salt, opts);
//#endregion
//#region ../resolver/names.js
const utf8 = new TextEncoder();
const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const PDA_PREFIX = pad32("/LEE/v0.2/AccountId/PDA/");
const DERIVE_SALT = "logos-names/v5";
const DERIVE_PARAMS = {
	t: 3,
	m: 65536,
	p: 1,
	dkLen: 32,
	version: 19,
	asyncTick: 50
};
const ID_DOMAIN = "logos-names/v5/id";
const KEY_DOMAIN = "logos-names/v5/key";
const NAME_SEED_DOMAIN = "logos-names/v5/name/";
const PAGE_SEED_DOMAIN = "logos-names/v5/page/";
const ASSOCIATED_DATA = {
	record: "logos-names/v5/record",
	page: "logos-names/v5/page"
};
const NONCE_LENGTH = 12;
const PROFILE_BLOCK = 256;
const NAME_PATTERN = /^(?!-)[a-z0-9-]{1,32}(?<!-)$/;
/** How long an expired name still resolves, in milliseconds: 30 days. */
const GRACE_MS = 2592e6;
function pad32(text) {
	const bytes = /* @__PURE__ */ new Uint8Array(32);
	bytes.set(utf8.encode(text));
	return bytes;
}
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
function base58Encode(bytes) {
	let value = 0n;
	for (const byte of bytes) value = value << 8n | BigInt(byte);
	let text = "";
	while (value > 0n) {
		text = BASE58[Number(value % 58n)] + text;
		value /= 58n;
	}
	for (const byte of bytes) {
		if (byte !== 0) break;
		text = "1" + text;
	}
	return text;
}
function base58Decode(text) {
	let value = 0n;
	for (const char of text) {
		const digit = BASE58.indexOf(char);
		if (digit < 0) throw new Error(`invalid base58 character: ${char}`);
		value = value * 58n + BigInt(digit);
	}
	const bytes = [];
	while (value > 0n) {
		bytes.unshift(Number(value & 255n));
		value >>= 8n;
	}
	for (const char of text) {
		if (char !== "1") break;
		bytes.unshift(0);
	}
	return Uint8Array.from(bytes);
}
function accountIdBytes(accountId) {
	const bytes = base58Decode(accountId);
	if (bytes.length !== 32) throw new Error(`account ID must be 32 bytes: ${accountId}`);
	return bytes;
}
const toHex = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
function fromHex(text) {
	if (!/^([0-9a-f]{2})*$/i.test(text)) throw new Error(`invalid hex: ${text}`);
	return Uint8Array.from(text.match(/../g) ?? [], (pair) => parseInt(pair, 16));
}
/** How names are written for people: bare, `jef` (SPEC 2). */
const NAME_SUFFIX = "";
/** The old suffix, still accepted and removed when typed or pasted. */
const LEGACY_SUFFIX = ".lez";
/**
* Turns what a person typed, such as `Jef.lez`, into the bare name `jef`. Only
* ASCII letters are lowered, so no other character can turn into a valid one.
* The result still has to pass `isValidName`.
*/
function normalizeName(input) {
	const lowered = String(input).trim().replace(/[A-Z]/g, (letter) => letter.toLowerCase());
	let bare = lowered.startsWith("lns:") ? lowered.slice(4) : lowered;
	if (bare.startsWith("@")) bare = bare.slice(1);
	return bare.endsWith(".lez") ? bare.slice(0, -4) : bare;
}
function isValidName(name) {
	return typeof name === "string" && NAME_PATTERN.test(name);
}
/**
* The identifier and key of `name`. The chain knows the name only by `id`;
* `key` opens and seals what the name resolves to and is never sent anywhere.
* The derivation is slow and memory-hard on purpose, to make scanning for names
* expensive: about half a second and 64 MiB in pure JavaScript.
*/
async function deriveName(name) {
	if (!isValidName(name)) throw new Error(`invalid name: ${name}`);
	const master = await argon2idAsync(utf8.encode(name), utf8.encode(DERIVE_SALT), DERIVE_PARAMS);
	return {
		id: await sha256(concat(utf8.encode(ID_DOMAIN), master)),
		key: await sha256(concat(utf8.encode(KEY_DOMAIN), master))
	};
}
async function derivedAddress(program, domain, id) {
	if (!(id instanceof Uint8Array) || id.length !== 32) throw new Error("name ID must be 32 bytes");
	const seed = await sha256(concat(utf8.encode(domain), id));
	return base58Encode(await sha256(concat(PDA_PREFIX, accountIdBytes(program), seed)));
}
/** Account holding the record of the name `id` under the registry `program`. */
function recordAddress(program, id) {
	return derivedAddress(program, NAME_SEED_DOMAIN, id);
}
/** Account holding the page of the name `id` under the registry `program`. */
function pageAddress(program, id) {
	return derivedAddress(program, PAGE_SEED_DOMAIN, id);
}
function sealingKey(key, usage) {
	return crypto.subtle.importKey("raw", key, "AES-GCM", false, [usage]);
}
function sealingParams(nonce, kind) {
	const associated = ASSOCIATED_DATA[kind];
	if (!associated) throw new Error(`unknown kind of sealed data: ${kind}`);
	return {
		name: "AES-GCM",
		iv: nonce,
		additionalData: utf8.encode(associated)
	};
}
/**
* Seals `plaintext` under a name's `key` as a "record" or a "page": the nonce,
* then the ciphertext and its tag. The nonce is random; only tests pass one.
*/
async function seal(key, plaintext, kind, nonce = null) {
	nonce ??= crypto.getRandomValues(new Uint8Array(NONCE_LENGTH));
	if (nonce.length !== NONCE_LENGTH) throw new Error(`nonce must be ${NONCE_LENGTH} bytes`);
	const params = sealingParams(nonce, kind);
	const sealed = await crypto.subtle.encrypt(params, await sealingKey(key, "encrypt"), plaintext);
	return concat(nonce, new Uint8Array(sealed));
}
/** The plaintext of `sealed`, or null when it does not open with `key` as that kind. */
async function openSealed(key, sealed, kind) {
	const params = sealingParams(sealed.subarray(0, NONCE_LENGTH), kind);
	if (sealed.length < 28) return null;
	try {
		const opening = await sealingKey(key, "decrypt");
		const ciphertext = sealed.subarray(NONCE_LENGTH);
		return new Uint8Array(await crypto.subtle.decrypt(params, opening, ciphertext));
	} catch {
		return null;
	}
}
/** Minimal borsh reader for the registry's layouts. */
var Reader = class {
	constructor(bytes) {
		this.bytes = bytes;
		this.offset = 0;
	}
	take(length) {
		if (this.offset + length > this.bytes.length) throw new Error("record is truncated");
		const slice = this.bytes.subarray(this.offset, this.offset + length);
		this.offset += length;
		return slice;
	}
	u8() {
		return this.take(1)[0];
	}
	u32() {
		const [a, b, c, d] = this.take(4);
		return (a | b << 8 | c << 16 | d << 24) >>> 0;
	}
	u64() {
		const low = this.u32();
		return this.u32() * 2 ** 32 + low;
	}
	byteVec() {
		return this.take(this.u32());
	}
	string() {
		return new TextDecoder("utf-8", { fatal: true }).decode(this.byteVec());
	}
	option(read) {
		return this.u8() === 0 ? null : read();
	}
};
/** Minimal borsh writers, the counterparts of the reader. */
const writeU32 = (value) => Uint8Array.of(value, value >>> 8, value >>> 16, value >>> 24);
const writeByteVec = (bytes) => concat(writeU32(bytes.length), bytes);
const writeString = (text) => writeByteVec(utf8.encode(text));
const writeOption = (bytes) => bytes === null ? Uint8Array.of(0) : concat(Uint8Array.of(1), bytes);
function fixed32(bytes, what) {
	if (bytes.length !== 32) throw new Error(`${what} must be 32 bytes`);
	return bytes;
}
/**
* The tag that stands for the account `accountId` as the holder of the name
* `id` (32 bytes). A record keeps this in place of its holder, so it names no
* account and links no two names.
*/
async function ownerTag(id, accountId) {
	return toHex(await sha256(concat(utf8.encode("logos-names/v5/owner"), id, base58Decode(accountId))));
}
function decodeNameRecord(bytes) {
	const reader = new Reader(bytes);
	return {
		version: reader.u8(),
		ownerTag: toHex(reader.take(32)),
		expires: reader.u64(),
		sealed: reader.byteVec()
	};
}
function decodePage(bytes) {
	const reader = new Reader(bytes);
	return {
		version: reader.u8(),
		sealed: reader.byteVec()
	};
}
function decodePrimaryName(bytes) {
	const reader = new Reader(bytes);
	return {
		version: reader.u8(),
		name: reader.string()
	};
}
/** Decodes the profile at the start of an opened record; the zero padding after it is ignored. */
function decodeProfile(bytes) {
	const reader = new Reader(bytes);
	const profile = {
		name: reader.string(),
		account: reader.option(() => base58Encode(reader.take(32))),
		npk: reader.option(() => toHex(reader.take(32))),
		vpk: toHex(reader.byteVec()) || null,
		texts: {}
	};
	const count = reader.u32();
	for (let index = 0; index < count; index += 1) {
		const key = reader.string();
		profile.texts[key] = reader.string();
	}
	return profile;
}
/** Encodes a profile, padded with zeros to a whole number of blocks so its length says little. */
function encodeProfile({ name, account = null, npk = null, vpk = null, texts = {} }) {
	const entries = Object.entries(texts);
	const encoded = concat(writeString(name), writeOption(account === null ? null : accountIdBytes(account)), writeOption(npk === null ? null : fixed32(fromHex(npk), "npk")), writeByteVec(vpk === null ? /* @__PURE__ */ new Uint8Array(0) : fromHex(vpk)), writeU32(entries.length), ...entries.flatMap(([key, value]) => [writeString(key), writeString(value)]));
	const padded = new Uint8Array(Math.ceil(encoded.length / PROFILE_BLOCK) * PROFILE_BLOCK);
	padded.set(encoded);
	return padded;
}
function decodePageContent(bytes) {
	const reader = new Reader(bytes);
	return {
		contentType: reader.string(),
		body: reader.byteVec()
	};
}
function encodePageContent({ contentType, body }) {
	return concat(writeString(contentType), writeByteVec(body));
}
/** Reads the registry's shard on `accountId`; empty when the account holds none. */
async function readShard(rpcUrl, program, accountId) {
	const response = await fetch(rpcUrl, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			jsonrpc: "2.0",
			id: 1,
			method: "getAccount",
			params: [accountId]
		})
	});
	if (!response.ok) throw new Error(`sequencer returned HTTP ${response.status}`);
	const { result, error } = await response.json();
	if (error) throw new Error(`sequencer error: ${error.message}`);
	return Uint8Array.from(result?.data?.shards?.[program] ?? []);
}
/** Opens and decodes `sealed`, or null when it is empty, does not open or does not decode. */
async function openAs(key, sealed, kind, decode) {
	if (sealed.length === 0) return null;
	const plaintext = await openSealed(key, sealed, kind);
	if (plaintext === null) return null;
	try {
		return decode(plaintext);
	} catch {
		return null;
	}
}
/** The record of the name `id`, or null when it is not registered or is past grace at `now`. */
async function readRecord(rpcUrl, program, id, now) {
	const bytes = await readShard(rpcUrl, program, await recordAddress(program, id));
	if (bytes.length === 0) return null;
	const record = decodeNameRecord(bytes);
	return now >= record.expires + 2592e6 ? null : record;
}
/**
* What `name` resolves to at the time `now`, or null when it is not registered
* or is past its grace period. `expires` is in milliseconds since the Unix
* epoch; `lapsed` is true for a name that has expired and is in its grace
* period, which still resolves. A record whose sealed profile is empty, does
* not open with the name's key or names another name is a registered name with
* nothing set.
*/
async function resolveName(rpcUrl, program, name, now = Date.now()) {
	const { id, key } = await deriveName(name);
	const record = await readRecord(rpcUrl, program, id, now);
	if (record === null) return null;
	const { sealed } = record;
	const expires = record.expires >= Number.MAX_SAFE_INTEGER ? null : record.expires;
	const held = {
		name,
		expires,
		lapsed: expires !== null && expires <= now
	};
	const profile = await openAs(key, sealed, "record", decodeProfile);
	if (profile === null || profile.name !== name) return {
		...held,
		account: null,
		npk: null,
		vpk: null,
		texts: {}
	};
	const { account, npk, vpk, texts } = profile;
	return {
		...held,
		account,
		npk,
		vpk,
		texts
	};
}
/**
* What a name shows people: whether it can be paid privately, and the public
* account only if its holder published one. The private receiving keys are
* never shown; a payer's wallet uses them from `resolveName`.
*/
function publicView(record) {
	const { name, expires, lapsed, account, npk, vpk, texts } = record;
	return {
		name,
		expires,
		lapsed,
		payablePrivately: Boolean(npk && vpk),
		publicAccount: account,
		texts
	};
}
/**
* The page for `name` at the time `now`, or null when the name is not
* registered or is past its grace period, has no page, or the page does not
* open with the name's key.
*/
async function resolvePage(rpcUrl, program, name, now = Date.now()) {
	const { id, key } = await deriveName(name);
	if (await readRecord(rpcUrl, program, id, now) === null) return null;
	const bytes = await readShard(rpcUrl, program, await pageAddress(program, id));
	if (bytes.length === 0) return null;
	return openAs(key, decodePage(bytes).sealed, "page", decodePageContent);
}
/**
* The name `accountId` is shown as at the time `now`, or null. A reverse record
* is only a claim, so it counts only when the forward record points back at the
* same account.
*/
async function reverseLookup(rpcUrl, program, accountId, now = Date.now()) {
	const bytes = await readShard(rpcUrl, program, accountId);
	if (bytes.length === 0) return null;
	const { name } = decodePrimaryName(bytes);
	if (!isValidName(name)) return null;
	const { id, key } = await deriveName(name);
	const record = await readRecord(rpcUrl, program, id, now);
	if (record === null) return null;
	if (record.ownerTag === await ownerTag(id, accountId)) return name;
	const profile = await openAs(key, record.sealed, "record", decodeProfile);
	return profile !== null && profile.name === name && profile.account === accountId ? name : null;
}
//#endregion
export { GRACE_MS, LEGACY_SUFFIX, NAME_SUFFIX, base58Decode, base58Encode, decodeNameRecord, decodePage, decodePageContent, decodePrimaryName, decodeProfile, deriveName, encodePageContent, encodeProfile, isValidName, normalizeName, openSealed, ownerTag, pageAddress, publicView, readShard, recordAddress, resolveName, resolvePage, reverseLookup, seal };
