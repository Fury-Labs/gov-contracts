use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::{Addr, BlockInfo, Decimal, StdResult, Storage, Uint128, Coin};
use cw3::{Status, Vote};
use cw_storage_plus::{Item, Map};
use cw_utils::{ Expiration, Threshold};
use comdex_bindings::ComdexMessages;

// we multiply by this when calculating needed_votes in order to round up properly
// Note: `10u128.pow(9)` fails as "u128::pow` is not yet stable as a const fn"
const PRECISION_FACTOR: u128 = 1_000_000_000;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Config {
    pub threshold: Threshold,
   
    pub target:String,
   
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]

pub struct Proposal {
    pub title: String,
    pub description: String,
    pub start_height: u64,
    pub expires: Expiration,
    pub msgs: Vec<ComdexMessages>,
    pub status: Status,
    /// pass requirements
    pub threshold: Threshold,
    // the total weight when the proposal started (used to calculate percentages)
    pub total_weight: u128,
    // summary of existing votes
    pub votes: Votes,
    pub deposit :Vec<Coin>,
    pub proposer : String,
    pub token_denom :String,
    pub deposit_refunded: bool,
    pub min_deposit:u64,
    pub deposit_denom:String,
    pub current_deposit:u128
}

impl Proposal {
    /// current_status is non-mutable and returns what the status should be.
    /// (designed for queries)
    pub fn current_status(&self, block: &BlockInfo) -> Status {
        let mut status = self.status;

        // if open, check if voting is passed or timed out
        if status == Status::Open && self.is_passed(block) {
            status = Status::Passed;
        }
        if status == Status::Open && (self.is_rejected(block) || self.expires.is_expired(block)) {
            status = Status::Rejected;
        }

        status
    }

    /// update_status sets the status of the proposal to current_status.
    /// (designed for handler logic)
    pub fn update_status(&mut self, block: &BlockInfo) {
        self.status = self.current_status(block);
    }

    /// Returns true if this proposal is sure to pass (even before expiration, if no future
    /// sequence of possible votes could cause it to fail).
    pub fn is_passed(&self, block: &BlockInfo) -> bool {
        match self.threshold {
            Threshold::AbsoluteCount {
                weight: weight_needed,
            } => self.votes.yes >= weight_needed,
            Threshold::AbsolutePercentage {
                percentage: percentage_needed,
            } => {
                self.votes.yes
                    >= votes_needed(self.total_weight - self.votes.abstain, percentage_needed)
            }
            Threshold::ThresholdQuorum { threshold, quorum } => {
                // we always require the quorum
                if self.votes.total() < votes_needed(self.total_weight, quorum) {
                    return false;
                }
                if self.expires.is_expired(block) {
                    // If expired, we compare vote_count against the total number of votes (minus abstain).
                    let opinions = self.votes.total() - self.votes.abstain;
                    self.votes.yes >= votes_needed(opinions, threshold)
                } else {
                    // If not expired, we must assume all non-votes will be cast against
                    let possible_opinions = self.total_weight - self.votes.abstain;
                    self.votes.yes >= votes_needed(possible_opinions, threshold)
                }
            }
        }
    }

    /// Returns true if this proposal is sure to be rejected (even before expiration, if
    /// no future sequence of possible votes could cause it to pass).
    pub fn is_rejected(&self, block: &BlockInfo) -> bool {
        match self.threshold {
            Threshold::AbsoluteCount {
                weight: weight_needed,
            } => {
                let weight = self.total_weight - weight_needed;
                self.votes.no > weight
            }
            Threshold::AbsolutePercentage {
                percentage: percentage_needed,
            } => {
                self.votes.no
                    > votes_needed(
                        self.total_weight - self.votes.abstain,
                        Decimal::one() - percentage_needed,
                    )
            }
            Threshold::ThresholdQuorum {
                threshold,
                quorum: _,
            } => {
                if self.expires.is_expired(block) {
                    // If expired, we compare vote_count against the total number of votes (minus abstain).
                    let opinions = self.votes.total() - self.votes.abstain;
                    self.votes.no > votes_needed(opinions, Decimal::one() - threshold)
                } else {
                    // If not expired, we must assume all non-votes will be cast for
                    let possible_opinions = self.total_weight - self.votes.abstain;
                    self.votes.no > votes_needed(possible_opinions, Decimal::one() - threshold)
                }
            }
        }
    }
}

// weight of votes for each option
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Votes {
    pub yes: u128,
    pub no: u128,
    pub abstain: u128,
    pub veto: u128,
}

impl Votes {
    /// sum of all votes
    pub fn total(&self) -> u128 {
        self.yes + self.no + self.abstain + self.veto
    }

    /// create it with a yes vote for this much
    pub fn yes(init_weight: u128) -> Self {
        Votes {
            yes: init_weight,
            no: 0,
            abstain: 0,
            veto: 0,
        }
    }

    pub fn add_vote(&mut self, vote: Vote, weight: u128) {
        match vote {
            Vote::Yes => self.yes += weight,
            Vote::Abstain => self.abstain += weight,
            Vote::No => self.no += weight,
            Vote::Veto => self.veto += weight,
        }
    }
}

// this is a helper function so Decimal works with u64 rather than Uint128
// also, we must *round up* here, as we need 8, not 7 votes to reach 50% of 15 total
fn votes_needed(weight: u128, percentage: Decimal) -> u128 {
    let applied = percentage * Uint128::new(PRECISION_FACTOR * weight as u128);
    // Divide by PRECISION_FACTOR, rounding up to the nearest integer
    ((applied.u128() + PRECISION_FACTOR - 1) / PRECISION_FACTOR) as u128
}

// we cast a ballot with our chosen vote and a given weight
// stored under the key that voted
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Ballot {
    pub weight: u128,
    pub vote: Vote,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct VoteWeight {
    pub yes: u128,
    pub no: u128,
    pub abstain: u128,
    pub veto: u128,

}

// unique items
pub const CONFIG: Item<Config> = Item::new("config");
pub const PROPOSAL_COUNT: Item<u64> = Item::new("proposal_count");

// multiple-item map
pub const BALLOTS: Map<(u64, &Addr), Ballot> = Map::new("votes");
pub const PROPOSALSBYAPP: Map<u64, Vec<u64>> = Map::new("ProposalsByApp");
pub const PROPOSALS: Map<u64, Proposal> = Map::new("proposals");
pub const VOTERDEPOSIT: Map<(u64, &Addr), Vec<Coin>> = Map::new("voter deposit");
pub const PROPOSALVOTE: Map<u64,VoteWeight> = Map::new("vote weight");


pub fn next_id(store: &mut dyn Storage) -> StdResult<u64> {
    let id: u64 = PROPOSAL_COUNT.may_load(store)?.unwrap_or_default() + 1;
    PROPOSAL_COUNT.save(store, &id)?;
    Ok(id)
}

