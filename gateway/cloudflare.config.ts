import { defineConfig } from "cf/config";

// Static files only (public/): the guide, install.sh and the MCP server. No code runs
// per request, so a request costs nothing and nothing here learns which names anyone
// looks up; names are resolved on the user's machine (`names`, `names open`).
export default defineConfig({
	worker: {
		name: "logos-names",
		compatibilityDate: "2026-10-01",
	},
});
