import assert from "node:assert/strict";
import { test } from "node:test";
import { nameCard, nameCardDataUrl } from "./card.js";

test("a 9-character name is drawn large on one line", () => {
  const svg = nameCard("jefdiesel");
  assert.match(svg, /<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg" viewBox="0 0 480 480"/);
  assert.match(svg, /font-size="60"/);
  assert.equal((svg.match(/<tspan /g) ?? []).length, 1);
  assert.match(svg, /<tspan x="40" y="190">jefdiesel<\/tspan>/);
  assert.match(svg, /fill="#152521"/);
  assert.match(svg, /fill="#ffd328"/);
  assert.match(svg, /nameswallet\.app/);
  assert.match(svg, /aria-label="LNS name card for jefdiesel"/);
});

test("a 32-character name is drawn small on at most two lines", () => {
  const name = "abcdefghijklmnopqrstuvwxyz012345";
  assert.equal(name.length, 32);
  const svg = nameCard(name);
  assert.match(svg, /font-size="36"/);
  const lines = [...svg.matchAll(/<tspan x="40" y="[\d.]+">(.*?)<\/tspan>/g)].map((m) => m[1]);
  assert.ok(lines.length <= 2);
  assert.equal(lines.join(""), name);
  assert.match(svg, /y="172"/);
});

test("a name is escaped even though the name rules forbid <&>", () => {
  const svg = nameCard("a<b&c>d");
  assert.match(svg, /a&lt;b&amp;c&gt;d/);
  assert.doesNotMatch(svg, /<b&c>/);
});

test("12 characters use the big size, 13 use the small size", () => {
  assert.match(nameCard("a".repeat(12)), /font-size="60"/);
  assert.match(nameCard("a".repeat(13)), /font-size="36"/);
});

test("a card data URL round-trips to the SVG", () => {
  const prefix = "data:image/svg+xml;charset=utf-8,";
  const url = nameCardDataUrl("jefdiesel");
  assert.ok(url.startsWith(prefix));
  assert.equal(decodeURIComponent(url.slice(prefix.length)), nameCard("jefdiesel"));
});
