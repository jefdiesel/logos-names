//! What a private job costs, checked before its first proof.
//!
//! A private change pays its fee inside its proof, from a private account the LEZ wallet
//! picks for each change (`auto_fee_payer` and `send_proven` in lez/wallet/src/lib.rs,
//! v0.4.0-rc1). The payer is:
//! - the account the job names (`names receive --fee-from`), else
//! - the account the change acts for, when its note holds the fee reserve (`at_most`), else
//! - the first private account of the wallet whose note holds the reserve, else
//! - the account the change acts for, paying out of what the change credits it (a move to
//!   private into an empty account).
//!
//! The payer's waiting credits (its fees' refunds, at most four, largest first) are swept into
//! the proof and count as what it holds. The reserve is `at_most`, or what the payer holds when
//! that is less, but never less than the fee at the head (`about`). A payer left with less than
//! the reserve once the change has moved what it moves is refused by the wallet before anything
//! is sent ("Fee reserve of … fails"). The surplus comes back as a new waiting credit.
//!
//! Every private job is checked here with those rules, before its first proof. (Each job used
//! to check one account against one fee: it refused what the wallet would pay for, and let
//! through jobs whose second or third step the wallet then refused, half done.)
//! - A job of one step is checked as the wallet checks it, so it is refused here exactly when
//!   the wallet would refuse it. Refused by the wallet, it costs nothing: nothing is sent.
//! - A job of several steps is checked at its worst case, every step's fee at `at_most` and no
//!   refund counted: a step that fails after others landed leaves the job half done with their
//!   fees spent. Such a job either can pay for all its steps, or doesn't start.

use crate::fees::Estimate;

/// A private account of the wallet as a job starts: its note, and the credits waiting for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holding<K> {
    pub id: K,
    pub note: u128,
    pub credits: Vec<u128>,
}

/// One private change of a job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step<K> {
    /// What it does, in words: "moving 14,000,000 to Private/…".
    pub what: String,
    /// The private account the change acts for (its signer).
    pub acts_for: K,
    /// What the change moves out of `acts_for` (a deposit, a transfer).
    pub moves_out: u128,
    /// What the change credits `acts_for` before the fee is taken (a move to private, a claim).
    pub credits: u128,
    /// The payer the job names, if it names one.
    pub payer: Option<K>,
    /// Its fee (`fees::private_running`).
    pub fee: Estimate,
}

/// Why a job can't start: step `step` (from 0) would be refused, paid by `payer`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Short<K> {
    pub step: usize,
    pub payer: K,
    /// What `payer` holds before the step, its waiting credits included.
    pub holds: u128,
    /// What the step credits the payer before its fee (it acts for the payer).
    pub gets: u128,
    /// What the step moves out of the account it acts for.
    pub sends: u128,
    /// Whether the step acts for its payer, so what it moves comes out of the payer too.
    pub own: bool,
    /// The fee the step reserves from the payer.
    pub fee: u128,
    /// The account the step acts for and what it holds, when it can't send what it moves.
    pub sender_short: Option<(K, u128)>,
}

impl<K: Copy> Short<K> {
    /// How much more this one step needs, and in which account.
    pub fn missing(&self) -> (K, u128) {
        if let Some((sender, holds)) = self.sender_short {
            return (sender, self.sends.saturating_sub(holds));
        }
        let sends = if self.own { self.sends } else { 0 };
        (self.payer, self.fee.saturating_add(sends).saturating_sub(self.holds.saturating_add(self.gets)))
    }
}

/// What a job that can start costs: who pays each step, and its fees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Costed<K> {
    pub payers: Vec<K>,
    /// Its fees, every step at the fee at the head.
    pub about: u128,
    /// Its fees, every step at `at_most`.
    pub at_most: u128,
    /// What its steps move out of the accounts they act for.
    pub moved: u128,
}

/// The largest credits first, at most this many: what one change sweeps into its payer.
const SWEPT: usize = 4;

/// Runs `steps` over `holdings` as the wallet would; `worst`: every fee and reserve at `at_most`.
fn run<K: Copy + Eq>(holdings: &[Holding<K>], steps: &[Step<K>], worst: bool) -> Result<Vec<K>, Short<K>> {
    let mut held: Vec<Holding<K>> = holdings
        .iter()
        .map(|holding| {
            let mut credits = holding.credits.clone();
            credits.sort_unstable_by(|a, b| b.cmp(a));
            Holding { id: holding.id, note: holding.note, credits }
        })
        .collect();
    let note = |held: &[Holding<K>], id: K| held.iter().find(|holding| holding.id == id).map_or(0, |holding| holding.note);
    let mut payers = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        let fee = step.fee;
        let payer = step
            .payer
            .or_else(|| (note(&held, step.acts_for) >= fee.at_most).then_some(step.acts_for))
            .or_else(|| held.iter().find(|holding| holding.note >= fee.at_most).map(|holding| holding.id))
            .unwrap_or(step.acts_for);
        // A change sweeps the waiting credits of its payer and of the account it acts for.
        let sweeping = if payer == step.acts_for { vec![payer] } else { vec![payer, step.acts_for] };
        for id in sweeping {
            if let Some(holding) = held.iter_mut().find(|holding| holding.id == id) {
                let take = holding.credits.len().min(SWEPT);
                let swept: u128 = holding.credits.drain(..take).sum();
                holding.note = holding.note.saturating_add(swept);
            }
        }
        let own = payer == step.acts_for;
        let holds = note(&held, payer);
        let (gets, sends) = if own { (step.credits, step.moves_out) } else { (0, 0) };
        let reserve = if worst { fee.at_most } else { fee.at_most.min(holds).max(fee.about) };
        let charged = if worst { fee.at_most } else { fee.about };
        let short = |sender_short| Short { step: index, payer, holds, gets, sends: step.moves_out, own, fee: reserve, sender_short };
        if !own {
            let sender = note(&held, step.acts_for);
            if sender < step.moves_out {
                return Err(short(Some((step.acts_for, sender))));
            }
        }
        if holds.saturating_add(gets) < reserve.saturating_add(sends) {
            return Err(short(None));
        }
        for holding in &mut held {
            if holding.id == step.acts_for {
                holding.note = holding.note.saturating_add(step.credits).saturating_sub(step.moves_out);
            }
            if holding.id == payer {
                holding.note = holding.note.saturating_sub(reserve);
                // The reserve's surplus comes back as a credit waiting for the payer.
                if reserve > charged {
                    holding.credits.push(reserve - charged);
                    holding.credits.sort_unstable_by(|a, b| b.cmp(a));
                }
            }
        }
        payers.push(payer);
    }
    Ok(payers)
}

/// Checks a job of `steps` over the wallet's private `holdings`: one step as the wallet checks
/// it, several at their worst case (see the module's comment).
pub fn check<K: Copy + Eq>(holdings: &[Holding<K>], steps: &[Step<K>]) -> Result<Costed<K>, Short<K>> {
    let payers = run(holdings, steps, steps.len() > 1)?;
    Ok(Costed {
        payers,
        about: steps.iter().map(|step| step.fee.about).sum(),
        at_most: steps.iter().map(|step| step.fee.at_most).sum(),
        moved: steps.iter().map(|step| step.moves_out).sum(),
    })
}

/// How many of `steps`, from the first, can be paid for, each set checked as [`check`] does.
pub fn fitting<K: Copy + Eq>(holdings: &[Holding<K>], steps: &[Step<K>]) -> usize {
    (1..=steps.len()).rev().find(|count| check(holdings, &steps[..*count]).is_ok()).unwrap_or(0)
}

/// The least that, added to what `short`'s account holds, lets the whole job start: found by
/// trying, so it counts the steps after the one that fell short too.
pub fn top_up<K: Copy + Eq>(holdings: &[Holding<K>], steps: &[Step<K>], short: &Short<K>) -> (K, u128) {
    let (who, step_only) = short.missing();
    let with = |extra: u128| {
        let mut more: Vec<Holding<K>> = holdings.to_vec();
        match more.iter_mut().find(|holding| holding.id == who) {
            Some(holding) => holding.note = holding.note.saturating_add(extra),
            None => more.push(Holding { id: who, note: extra, credits: Vec::new() }),
        }
        check(&more, steps).is_ok()
    };
    let mut high: u128 = steps.iter().map(|step| step.fee.at_most.saturating_add(step.moves_out)).sum();
    if !with(high) {
        return (who, step_only);
    }
    let mut low = step_only.min(high);
    if with(low) {
        return (who, low);
    }
    // `with(low)` fails and `with(high)` passes.
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if with(middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    (who, high)
}

/// Digits grouped in threes: 5070616 is 5,070,616.
pub fn grouped(amount: u128) -> String {
    let digits = amount.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Why a job can't start, in words. `job` names it ("registering alice"); `account` writes an
/// account (`Private/…`). Ends with "Nothing was sent."
pub fn refusal<K: Copy + Eq>(job: &str, holdings: &[Holding<K>], steps: &[Step<K>], short: &Short<K>, account: impl Fn(K) -> String) -> String {
    let step = &steps[short.step];
    let payer = account(short.payer);
    let mut text = String::new();
    if steps.len() > 1 {
        let about: u128 = steps.iter().map(|step| step.fee.about).sum();
        let at_most: u128 = steps.iter().map(|step| step.fee.at_most).sum();
        let moved: u128 = steps.iter().map(|step| step.moves_out).sum();
        text.push_str(&format!(
            "{job} takes {} private steps. Their fees are about {} in all, at most {} if fees climb{}. Step {} of {} ({}) could not be paid: ",
            steps.len(),
            grouped(about),
            grouped(at_most),
            if moved > 0 { format!(", and they move {} besides", grouped(moved)) } else { String::new() },
            short.step + 1,
            steps.len(),
            step.what
        ));
    } else {
        text.push_str(&format!("{} can't be paid: ", step.what));
    }
    if let Some((sender, holds)) = short.sender_short {
        text.push_str(&format!("{} holds {}, less than the {} it moves.", account(sender), grouped(holds), grouped(short.sends)));
    } else {
        let mut has = format!("{payer} pays the fee inside the proof and holds {}", grouped(short.holds));
        if short.gets > 0 {
            has.push_str(&format!(", and gets {} from this change", grouped(short.gets)));
        }
        let needs = if short.own && short.sends > 0 {
            format!("{} for what it moves and {} for the fee", grouped(short.sends), grouped(short.fee))
        } else {
            format!("{} for the fee", grouped(short.fee))
        };
        text.push_str(&format!("{has}; it needs {needs}{}.", if short.step > 0 { " by then" } else { "" }));
    }
    let (who, more) = top_up(holdings, steps, short);
    text.push_str(&format!(
        " Move at least {} more into {} first (`names setup --shield=<amount>`, or a payment to one of your names). Nothing was sent.",
        grouped(more.max(1)),
        account(who)
    ));
    text
}

/// The wallet's refusal to build a private change whose payer can't hold the fee reserve, in
/// plain words: such a change never reaches the sequencer, so nothing was sent for it.
pub fn build_refusal(error: &str) -> Option<String> {
    let rest = error.split("Fee reserve of ").nth(1)?;
    let reserve = rest.split_whitespace().next()?.parse::<u128>().ok()?;
    let payer = rest.split("from payer ").nth(1)?.split_whitespace().next()?.trim_end_matches(',');
    Some(format!(
        "the wallet refused this change before sending it: Private/{payer} pays its fee and can't hold the fee reserve of {} on top of what the change moves. Nothing was sent for it",
        grouped(reserve)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fee of a native transfer on the local rc1 chain these findings were made on.
    const FEE: Estimate = Estimate { about: 5_070_616, at_most: 8_239_751 };

    fn holding(id: char, note: u128, credits: &[u128]) -> Holding<char> {
        Holding { id, note, credits: credits.to_vec() }
    }

    fn step(acts_for: char, moves_out: u128, credits: u128, payer: Option<char>) -> Step<char> {
        Step { what: format!("step for {acts_for}"), acts_for, moves_out, credits, payer, fee: FEE }
    }

    fn account(id: char) -> String {
        format!("Private/{id}")
    }

    /// shield-refused-before-proving-though-wallet-would-pay (a): a move of 14,000,000 into an
    /// empty default account pays its fee out of what moves, as the wallet does.
    #[test]
    fn a_move_into_an_empty_account_pays_its_fee_from_what_moves() {
        let holdings = [holding('D', 0, &[])];
        let costed = check(&holdings, &[step('D', 0, 14_000_000, None)]).expect("the wallet proves it");
        assert_eq!(costed.payers, vec!['D']);
        // A move smaller than the fee can't pay it.
        let small = [step('D', 0, 3_000_000, None)];
        let short = check(&holdings, &small).unwrap_err();
        assert_eq!((short.payer, short.holds, short.gets, short.fee, short.missing()), ('D', 0, 3_000_000, FEE.about, ('D', 2_070_616)));
        assert_eq!(top_up(&holdings, &small, &short), ('D', 2_070_616));
        let text = refusal("moving to private", &holdings, &small, &short, account);
        assert_eq!(
            text,
            "step for D can't be paid: Private/D pays the fee inside the proof and holds 0, and gets 3,000,000 from this change; it needs 5,070,616 for the fee. Move at least 2,070,616 more into Private/D first (`names setup --shield=<amount>`, or a payment to one of your names). Nothing was sent."
        );
    }

    /// shield-refused-before-proving-though-wallet-would-pay (b): the default account E is
    /// empty and Q holds the reserve, so the wallet has Q pay; the check names Q too.
    #[test]
    fn another_private_account_holding_the_reserve_pays() {
        let holdings = [holding('Q', 8_929_384, &[]), holding('E', 0, &[])];
        assert_eq!(check(&holdings, &[step('E', 0, 14_000_000, None)]).unwrap().payers, vec!['Q']);
        // Even a move smaller than the fee: Q pays it.
        assert_eq!(check(&holdings, &[step('E', 0, 1_000, None)]).unwrap().payers, vec!['Q']);
    }

    /// multi-claim-precheck-checks-one-claim: P holds 8,929,384, enough for one claim (at most
    /// 8,239,751) but not for two at their worst case; one claim fits.
    #[test]
    fn claims_are_checked_together_and_as_many_as_fit_are_told() {
        let holdings = [holding('P', 8_929_384, &[]), holding('A', 0, &[]), holding('B', 0, &[])];
        let claims = [step('A', 0, 1_000_000, Some('P')), step('B', 0, 1_000_000, Some('P'))];
        let short = check(&holdings, &claims).unwrap_err();
        assert_eq!((short.step, short.payer, short.holds, short.fee), (1, 'P', 689_633, FEE.at_most));
        assert_eq!(fitting(&holdings, &claims), 1);
        // Two claims fit with twice the worst case, the refunds waiting for P counted.
        let rich = [holding('P', 10_000_000, &[6_500_000]), holding('A', 0, &[]), holding('B', 0, &[])];
        assert_eq!(fitting(&rich, &claims), 2);
        // One claim alone is checked as the wallet checks it: the fee at the head.
        let poor = [holding('P', 5_100_000, &[]), holding('A', 0, &[])];
        assert!(check(&poor, &claims[..1]).is_ok());
        assert_eq!(fitting(&[holding('P', 5_000_000, &[]), holding('A', 0, &[])], &claims), 0);
    }

    /// register-as-public-silently-stays-private: commit, reveal (deposit 1,000,000), funding
    /// the public account (10,000,000) and the hand-over, all from S. 29,289,152 pays commit and
    /// reveal but not the rest; the check refuses before the commit.
    #[test]
    fn registering_as_public_is_checked_whole_before_the_commit() {
        let holdings = [holding('S', 26_120_017, &[3_169_135])];
        let steps = [step('S', 0, 0, None), step('S', 1_000_000, 0, None), step('S', 10_000_000, 0, None), step('S', 0, 0, None)];
        let short = check(&holdings, &steps).unwrap_err();
        assert_eq!(short.step, 2, "{short:?}");
        assert_eq!(top_up(&holdings, &steps, &short), ('S', 4 * FEE.at_most + 11_000_000 - 29_289_152));
        let text = refusal("registering alice and making it public", &holdings, &steps, &short, account);
        assert!(text.starts_with("registering alice and making it public takes 4 private steps."), "{text}");
        assert!(text.contains("Step 3 of 4 (step for S) could not be paid"), "{text}");
        assert!(text.contains("Move at least 14,669,852 more into Private/S"), "{text}");
        assert!(text.ends_with("Nothing was sent."), "{text}");
        let enough = [holding('S', 4 * FEE.at_most + 11_000_000, &[])];
        assert!(check(&enough, &steps).is_ok());
        // Registering privately alone fits in what S holds.
        assert!(check(&holdings, &steps[..2]).is_ok());
    }

    /// The wallet's own rule: a change that moves money out of its payer needs that plus the
    /// whole reserve, even alone (the make-public funding step that failed in the finding).
    #[test]
    fn a_send_from_the_payer_needs_the_whole_reserve_besides() {
        let holdings = [holding('S', 14_109_098, &[3_473_150])];
        let fund = [step('S', 10_000_000, 0, None)];
        let short = check(&holdings, &fund).unwrap_err();
        assert_eq!((short.fee, short.sends, short.missing()), (FEE.at_most, 10_000_000, ('S', 657_503)));
        let text = refusal("", &holdings, &fund, &short, account);
        assert!(text.contains("holds 17,582,248; it needs 10,000,000 for what it moves and 8,239,751 for the fee."), "{text}");
        // S's note holds the reserve, so the wallet has S pay, though R could: refused as the
        // wallet would refuse it.
        let two = [holding('R', 9_000_000, &[]), holding('S', 10_000_000, &[])];
        assert_eq!(check(&two, &fund).unwrap_err().payer, 'S');
        // Below the reserve, S only sends and R, which holds it, pays the fee.
        let move_8m = [step('S', 8_000_000, 0, None)];
        assert_eq!(check(&[holding('R', 9_000_000, &[]), holding('S', 8_000_000, &[])], &move_8m).unwrap().payers, vec!['R']);
        let short = check(&[holding('R', 9_000_000, &[]), holding('S', 4_000_000, &[])], &fund).unwrap_err();
        assert_eq!(short.missing(), ('S', 6_000_000));
    }

    /// A dearer step (a registry call's public part) is checked at its own fee.
    #[test]
    fn each_step_is_checked_at_its_own_fee() {
        let reveal = Estimate { about: 5_811_525, at_most: 9_400_000 };
        let steps = [Step { fee: reveal, ..step('S', 0, 0, None) }];
        assert!(check(&[holding('S', 5_500_000, &[])], &steps).is_err());
        assert!(check(&[holding('S', 5_900_000, &[])], &steps).is_ok());
    }

    #[test]
    fn the_wallets_build_refusal_is_said_plainly() {
        let raw = "Failed to build transaction: Invalid input: Fee reserve of 5070616 from payer 5UWZabc fails, though the transaction runs without it: Invalid native transfer: sender 5UWZabc holds less than the transferred amount";
        let plain = build_refusal(raw).unwrap();
        assert!(plain.contains("Private/5UWZabc pays its fee and can't hold the fee reserve of 5,070,616"), "{plain}");
        assert!(plain.ends_with("Nothing was sent for it"));
        assert_eq!(build_refusal("the sequencer is down"), None);
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(29_289_152), "29,289,152");
    }
}
