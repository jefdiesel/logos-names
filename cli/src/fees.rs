//! What a change costs in fees, before and after it is sent, in lines the Basecamp module
//! reads (`fee: about A, at most M (…)` and `fee paid: P`). LEZ 0.4 keeps no receipt of
//! a transaction's fee, so what was paid is measured from the payer's balance.
//!
//! A public transaction holds `at most` from its payer while it runs and is billed what it
//! used: its cycles at the execution base fee and its bytes at the storage base fee. A
//! private one pays inside its proof, from a private account: a flat verification and
//! storage cost at the base fees of the block it lands in, `at most` being that cost after
//! the wallet's headroom of full blocks.

use fee_core::{assess::private_fee_required, market};
use sequencer_service_rpc::FeeStateQuote;

/// The bytes a transaction carries besides its message: signatures, nonces and actors.
pub const ENVELOPE_BYTES: u128 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Estimate {
    pub about: u128,
    pub at_most: u128,
}

/// A public call of `cycles` cycles under gas limit `limit`, with a `message_bytes` message.
pub fn public(quote: &FeeStateQuote, cycles: u64, limit: u64, message_bytes: usize) -> Estimate {
    let bytes = u128::try_from(message_bytes).unwrap_or(u128::MAX).saturating_add(ENVELOPE_BYTES);
    Estimate {
        about: u128::from(cycles) * u128::from(quote.base_fee_exec) + bytes * u128::from(quote.base_fee_stor),
        at_most: u128::from(limit) * u128::from(quote.next_base_fee_exec_ceiling)
            + bytes * u128::from(quote.next_base_fee_stor_ceiling),
    }
}

/// A private transaction, its fee paid inside the proof. `headroom_blocks` is the wallet's
/// `private_fee_headroom_blocks`: how many full blocks the reserve allows the fees to climb.
pub fn private(quote: &FeeStateQuote, headroom_blocks: u64) -> Estimate {
    private_running(quote, headroom_blocks, 0)
}

/// A private transaction whose public part runs `cycles` cycles (a registry call's does; a
/// native transfer's runs none), its fee paid inside the proof: the wallet prices its reserve
/// so (`reserve_pricing` in the LEZ wallet).
pub fn private_running(quote: &FeeStateQuote, headroom_blocks: u64, cycles: u64) -> Estimate {
    let climb = |base, max_gas, target, damping, lo, hi| {
        (0..headroom_blocks).fold(base, |fee, _| market::next_base_fee(fee, max_gas, target, damping, lo, hi))
    };
    let exec = climb(
        quote.base_fee_exec,
        market::MAX_GAS_EXEC,
        market::TARGET_GAS_EXEC,
        market::D_EXEC,
        market::BASE_FEE_EXEC_MIN,
        market::BASE_FEE_EXEC_MAX,
    );
    let stor = climb(
        quote.base_fee_stor,
        market::MAX_GAS_STOR,
        market::TARGET_GAS_STOR,
        market::D_STOR,
        market::BASE_FEE_STOR_MIN,
        market::BASE_FEE_STOR_MAX,
    );
    Estimate {
        about: private_fee_required(cycles, quote.base_fee_exec, quote.base_fee_stor),
        at_most: private_fee_required(cycles, exec, stor),
    }
}

/// A public native-token transfer: the native token meters no cycles, so only its bytes
/// are billed, under the small gas limit the CLI gives it.
pub fn native_public(quote: &FeeStateQuote, gas_limit: u64) -> Estimate {
    public(quote, 0, gas_limit, 0)
}

/// The line printed before a change is sent. `how` says what the estimate covers.
pub fn estimate_line(estimate: &Estimate, how: &str) -> String {
    format!("fee: about {}, at most {} ({how})", estimate.about, estimate.at_most)
}

/// The line printed once a change applied: what left the payer besides what the change
/// moved. `None` when that could not be told (the payer was paid meanwhile).
pub fn paid_line(before: u128, after: u128, moved: u128) -> Option<String> {
    let paid = before.checked_sub(after)?.checked_sub(moved)?;
    Some(format!("fee paid: {paid}"))
}

/// `paid_line` from balance reads that may have failed (`None`): a fee measured from a
/// balance that couldn't be read is unknown, never a number made from a 0 put in its place.
pub fn paid_line_of(before: Option<u128>, after: Option<u128>, moved: u128) -> Option<String> {
    match (before, after) {
        (Some(before), Some(after)) => paid_line(before, after, moved),
        _ => Some("fee paid: unknown".to_owned()),
    }
}

/// The fee market as JSON, with what common sends cost at it (`names module fee`).
pub fn quote_json(quote: &FeeStateQuote, headroom_blocks: u64, native_gas_limit: u64) -> serde_json::Value {
    let json = |estimate: Estimate| {
        serde_json::json!({ "about": estimate.about.to_string(), "at_most": estimate.at_most.to_string() })
    };
    serde_json::json!({
        "height": quote.height,
        "base_fee_exec": quote.base_fee_exec,
        "base_fee_stor": quote.base_fee_stor,
        "public_transfer": json(native_public(quote, native_gas_limit)),
        "private_transfer": json(private(quote, headroom_blocks)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote() -> FeeStateQuote {
        FeeStateQuote {
            height: 7,
            base_fee_exec: 8,
            base_fee_stor: 8,
            next_base_fee_exec_floor: 8,
            next_base_fee_exec_ceiling: 9,
            next_base_fee_stor_floor: 8,
            next_base_fee_stor_ceiling: 9,
            max_gas_exec: market::MAX_GAS_EXEC,
            max_gas_stor: market::MAX_GAS_STOR,
        }
    }

    /// The Basecamp module reads these lines (`basecamp-module/rust-lib/src/cli.rs`,
    /// `parse_fee`); a change in wording here breaks the app's fee display.
    #[test]
    fn the_lines_the_module_reads_keep_their_shape() {
        let estimate = public(&quote(), 100, 150, 24);
        assert_eq!(estimate, Estimate { about: 100 * 8 + 1_024 * 8, at_most: 150 * 9 + 1_024 * 9 });
        assert_eq!(
            estimate_line(&estimate, "gas: 100 cycles, limit 150"),
            "fee: about 8992, at most 10566 (gas: 100 cycles, limit 150)"
        );
        assert_eq!(paid_line(1_000, 400, 500).as_deref(), Some("fee paid: 100"));
        assert_eq!(paid_line(1_000, 1_200, 0), None, "paid meanwhile: not told");
        assert_eq!(paid_line(1_000, 900, 500), None, "moved more than left: not told");
        // A balance read that failed was a 0: after = 0 made the whole balance a "fee".
        assert_eq!(paid_line_of(Some(1_000), None, 0).as_deref(), Some("fee paid: unknown"));
        assert_eq!(paid_line_of(None, Some(400), 0).as_deref(), Some("fee paid: unknown"));
        assert_eq!(paid_line_of(Some(1_000), Some(400), 500).as_deref(), Some("fee paid: 100"));
    }

    #[test]
    fn a_private_reserve_covers_the_fee_after_its_headroom() {
        let none = private(&quote(), 0);
        assert_eq!(none.about, none.at_most);
        let five = private(&quote(), 5);
        assert_eq!(five.about, none.about);
        assert!(five.at_most > five.about);
        assert_eq!(five.about, private_fee_required(0, 8, 8));
        // A registry call's public part makes it dearer (a reveal paid 5,811,525 where a
        // transfer paid 5,070,616).
        let running = private_running(&quote(), 5, 165_000);
        assert!(running.about > five.about && running.at_most > five.at_most);
    }

    #[test]
    fn the_quote_json_carries_text_amounts() {
        let json = quote_json(&quote(), 5, 1_000);
        assert_eq!(json["height"], 7);
        assert_eq!(json["public_transfer"]["about"], (1_000 * 8).to_string());
        assert!(json["private_transfer"]["at_most"].is_string());
    }
}
