import { bindings, defineConfig } from "cf/config";
import * as entrypoint from "./worker.js" with { type: "cf-worker" };

// Deploy-time settings, read from the environment:
//   NAMES_PROGRAM  account ID of the deployed registry program (required)
//   LEZ_RPC_URL    sequencer endpoint (defaults to the public LEZ testnet)
//   ROOT_HOSTS     hosts served in path mode, e.g. the workers.dev host
//   FAUCET_URL     the chain's faucet; also turns on the /rpc and /faucet routes
const program = process.env.NAMES_PROGRAM;
if (!program) throw new Error("set NAMES_PROGRAM to the registry program's account ID");

export default defineConfig({
	worker: {
		name: "logos-names",
		compatibilityDate: "2026-10-01",
		entrypoint,
		env: {
			LEZ_RPC_URL: bindings.text(process.env.LEZ_RPC_URL ?? "https://testnet.lez.logos.co"),
			NAMES_PROGRAM: bindings.text(program),
			ROOT_HOSTS: bindings.text(process.env.ROOT_HOSTS ?? ""),
			FAUCET_URL: bindings.text(process.env.FAUCET_URL ?? ""),
		},
	},
});
