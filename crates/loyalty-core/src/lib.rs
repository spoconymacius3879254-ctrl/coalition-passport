//! Deterministic, dependency-light business rules for Coalition Passport.
//!
//! This crate intentionally has no blockchain, wallet, clock, or I/O access.
//! The future Anchor program supplies authenticated signers and PDAs; this core
//! supplies the checked state transitions and invariants those instructions use.

use std::{collections::BTreeMap, error::Error, fmt};

use serde::{Deserialize, Serialize};

/// Basis points denominator used by merchant earning rules.
pub const BASIS_POINTS_DENOMINATOR: u64 = 10_000;
/// The public tier level is encoded as `u8`.
pub const MAX_TIER_COUNT: usize = u8::MAX as usize;

/// A stable merchant namespace. The program layer will derive this from a PDA.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MerchantId(String);

impl MerchantId {
    /// Builds a non-empty merchant identifier suitable for map keys and PDAs.
    ///
    /// # Errors
    ///
    /// Returns [`LoyaltyError::EmptyMerchantId`] when the supplied value is
    /// empty after trimming.
    pub fn new(value: impl Into<String>) -> Result<Self, LoyaltyError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(LoyaltyError::EmptyMerchantId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Immutable merchant parameters used when that merchant accrues credits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Merchant {
    id: MerchantId,
    /// Credit awarded per receipt unit, in basis points.
    earn_bps: u16,
    /// Maximum merchant-local credit that can be earned during one epoch.
    daily_cap: u64,
    active: bool,
}

impl Merchant {
    /// Creates active merchant rules with a valid basis-point earning rate.
    ///
    /// # Errors
    ///
    /// Returns a configuration error for a zero/excessive rate or zero cap.
    pub fn new(id: MerchantId, earn_bps: u16, daily_cap: u64) -> Result<Self, LoyaltyError> {
        if earn_bps == 0 {
            return Err(LoyaltyError::ZeroEarnBps);
        }
        if u64::from(earn_bps) > BASIS_POINTS_DENOMINATOR {
            return Err(LoyaltyError::InvalidEarnBps { earn_bps });
        }
        if daily_cap == 0 {
            return Err(LoyaltyError::ZeroDailyCap);
        }
        Ok(Self {
            id,
            earn_bps,
            daily_cap,
            active: true,
        })
    }

    #[must_use]
    pub fn id(&self) -> &MerchantId {
        &self.id
    }
}

/// Ordered points thresholds. Passing a threshold grants its zero-based level.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TierSchedule {
    thresholds: Vec<u64>,
}

impl TierSchedule {
    /// Creates a strictly increasing, non-empty tier schedule.
    ///
    /// # Errors
    ///
    /// Returns a schedule validation error when thresholds are empty, zero,
    /// exceed the `u8` level capacity, or are ambiguous/non-increasing.
    pub fn new(thresholds: Vec<u64>) -> Result<Self, LoyaltyError> {
        if thresholds.is_empty() {
            return Err(LoyaltyError::EmptyTierSchedule);
        }
        if thresholds.len() > MAX_TIER_COUNT {
            return Err(LoyaltyError::TooManyTiers {
                maximum: MAX_TIER_COUNT,
            });
        }
        if thresholds.contains(&0) {
            return Err(LoyaltyError::ZeroTierThreshold);
        }
        if thresholds.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(LoyaltyError::NonIncreasingTierSchedule);
        }
        Ok(Self { thresholds })
    }

    /// Returns the number of thresholds reached, including zero for no tier.
    #[must_use]
    pub fn level_for(&self, streak_points: u64) -> u8 {
        self.thresholds
            .iter()
            .take_while(|&&threshold| streak_points >= threshold)
            .count()
            .try_into()
            .unwrap_or(u8::MAX)
    }

    #[must_use]
    pub fn thresholds(&self) -> &[u64] {
        &self.thresholds
    }
}

/// Coalition-wide limits that prevent a single receipt from dominating a tier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CoalitionRules {
    max_receipt_units: u64,
    tiers: TierSchedule,
}

impl CoalitionRules {
    /// Creates valid coalition-wide receipt and tier limits.
    ///
    /// # Errors
    ///
    /// Returns [`LoyaltyError::ZeroMaxReceiptUnits`] if a receipt could never
    /// contribute to a coalition streak.
    pub fn new(max_receipt_units: u64, tiers: TierSchedule) -> Result<Self, LoyaltyError> {
        if max_receipt_units == 0 {
            return Err(LoyaltyError::ZeroMaxReceiptUnits);
        }
        Ok(Self {
            max_receipt_units,
            tiers,
        })
    }

    #[must_use]
    pub fn max_receipt_units(&self) -> u64 {
        self.max_receipt_units
    }

    #[must_use]
    pub fn tiers(&self) -> &TierSchedule {
        &self.tiers
    }
}

/// Merchant-local state held inside a customer passport.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MerchantBalance {
    pub earned_units: u64,
    pub redeemed_units: u64,
    pub earned_this_epoch: u64,
    pub cap_epoch: u64,
    pub last_receipt_nonce: u64,
}

impl MerchantBalance {
    #[must_use]
    pub fn available_units(&self) -> u64 {
        // All mutations preserve the invariant; saturating is defensive for
        // deserialized/corrupt data and never invents a redeemable credit.
        self.earned_units.saturating_sub(self.redeemed_units)
    }
}

/// Customer-owned coalition state. Merchant balances are deliberately keyed by
/// `MerchantId`, preventing an accrual API from addressing another merchant by
/// a free-form balance index.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Passport {
    pub customer_id: String,
    pub total_visits: u64,
    pub streak_points: u64,
    balances: BTreeMap<MerchantId, MerchantBalance>,
}

impl Passport {
    /// Creates customer state with no merchant-local balances.
    ///
    /// # Errors
    ///
    /// Returns [`LoyaltyError::EmptyCustomerId`] for a blank customer ID.
    pub fn new(customer_id: impl Into<String>) -> Result<Self, LoyaltyError> {
        let customer_id = customer_id.into();
        if customer_id.trim().is_empty() {
            return Err(LoyaltyError::EmptyCustomerId);
        }
        Ok(Self {
            customer_id,
            total_visits: 0,
            streak_points: 0,
            balances: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn balance_for(&self, merchant_id: &MerchantId) -> MerchantBalance {
        self.balances.get(merchant_id).cloned().unwrap_or_default()
    }

    #[must_use]
    pub fn tier_level(&self, rules: &CoalitionRules) -> u8 {
        rules.tiers().level_for(self.streak_points)
    }

    /// Applies a receipt signed by `merchant`; callers cannot supply a separate
    /// balance identifier, so a merchant can mutate only its own ledger entry.
    ///
    /// # Errors
    ///
    /// Returns typed errors for an inactive merchant, invalid amount/nonce,
    /// exhausted cap, zero earned credit, or checked-arithmetic overflow. On
    /// every error, this method leaves passport state unchanged.
    pub fn accrue(
        &mut self,
        merchant: &Merchant,
        nonce: u64,
        amount_units: u64,
        epoch: u64,
        rules: &CoalitionRules,
    ) -> Result<Accrual, LoyaltyError> {
        if !merchant.active {
            return Err(LoyaltyError::MerchantInactive {
                merchant: merchant.id.clone(),
            });
        }
        if amount_units == 0 {
            return Err(LoyaltyError::ZeroReceipt);
        }

        // Do not insert an empty ledger entry until all validation and checked
        // calculations have succeeded. Failed first receipts must be atomic.
        let mut balance = self.balances.get(&merchant.id).cloned().unwrap_or_default();
        if nonce <= balance.last_receipt_nonce {
            return Err(LoyaltyError::NonceNotMonotonic {
                previous: balance.last_receipt_nonce,
                received: nonce,
            });
        }

        // A new epoch resets only cap accounting. It never resets a nonce or
        // accumulated/redeemed customer credit.
        let earned_this_epoch = if balance.cap_epoch == epoch {
            balance.earned_this_epoch
        } else {
            0
        };
        let remaining_cap = merchant.daily_cap.saturating_sub(earned_this_epoch);
        if remaining_cap == 0 {
            return Err(LoyaltyError::DailyCapExhausted {
                merchant: merchant.id.clone(),
                epoch,
            });
        }

        let raw_credit = amount_units
            .checked_mul(u64::from(merchant.earn_bps))
            .ok_or(LoyaltyError::ArithmeticOverflow)?
            / BASIS_POINTS_DENOMINATOR;
        let credit_units = raw_credit.min(remaining_cap);
        if credit_units == 0 {
            return Err(LoyaltyError::ZeroCredit);
        }
        let streak_delta = credit_units.min(rules.max_receipt_units());

        // Calculate every new value before writing any state so a checked
        // arithmetic failure leaves the passport exactly unchanged.
        let new_earned = balance
            .earned_units
            .checked_add(credit_units)
            .ok_or(LoyaltyError::ArithmeticOverflow)?;
        let new_epoch_earned = earned_this_epoch
            .checked_add(credit_units)
            .ok_or(LoyaltyError::ArithmeticOverflow)?;
        let new_visits = self
            .total_visits
            .checked_add(1)
            .ok_or(LoyaltyError::ArithmeticOverflow)?;
        let new_streak = self
            .streak_points
            .checked_add(streak_delta)
            .ok_or(LoyaltyError::ArithmeticOverflow)?;

        balance.earned_units = new_earned;
        balance.earned_this_epoch = new_epoch_earned;
        balance.cap_epoch = epoch;
        balance.last_receipt_nonce = nonce;
        self.balances.insert(merchant.id.clone(), balance);
        self.total_visits = new_visits;
        self.streak_points = new_streak;

        Ok(Accrual {
            merchant: merchant.id.clone(),
            nonce,
            epoch,
            credit_units,
            streak_delta,
            tier_level: self.tier_level(rules),
        })
    }

    /// Customer-side transition: it can debit one named merchant's available
    /// balance, but can never consume coalition streak points or another
    /// merchant's ledger through an implicit conversion.
    ///
    /// # Errors
    ///
    /// Returns typed errors for a zero request, unknown merchant balance,
    /// insufficient merchant-local credit, or checked-arithmetic overflow.
    pub fn redeem(
        &mut self,
        merchant_id: &MerchantId,
        units: u64,
    ) -> Result<Redemption, LoyaltyError> {
        if units == 0 {
            return Err(LoyaltyError::ZeroRedemption);
        }
        let balance = self.balances.get_mut(merchant_id).ok_or_else(|| {
            LoyaltyError::MerchantBalanceMissing {
                merchant: merchant_id.clone(),
            }
        })?;
        let available = balance.available_units();
        if units > available {
            return Err(LoyaltyError::InsufficientMerchantCredit {
                merchant: merchant_id.clone(),
                requested: units,
                available,
            });
        }
        balance.redeemed_units = balance
            .redeemed_units
            .checked_add(units)
            .ok_or(LoyaltyError::ArithmeticOverflow)?;
        Ok(Redemption {
            merchant: merchant_id.clone(),
            redeemed_units: units,
            remaining_units: balance.available_units(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Accrual {
    pub merchant: MerchantId,
    pub nonce: u64,
    pub epoch: u64,
    pub credit_units: u64,
    pub streak_delta: u64,
    pub tier_level: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Redemption {
    pub merchant: MerchantId,
    pub redeemed_units: u64,
    pub remaining_units: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoyaltyError {
    EmptyMerchantId,
    EmptyCustomerId,
    ZeroEarnBps,
    InvalidEarnBps {
        earn_bps: u16,
    },
    ZeroDailyCap,
    EmptyTierSchedule,
    ZeroTierThreshold,
    TooManyTiers {
        maximum: usize,
    },
    NonIncreasingTierSchedule,
    ZeroMaxReceiptUnits,
    MerchantInactive {
        merchant: MerchantId,
    },
    ZeroReceipt,
    ZeroCredit,
    ZeroRedemption,
    NonceNotMonotonic {
        previous: u64,
        received: u64,
    },
    DailyCapExhausted {
        merchant: MerchantId,
        epoch: u64,
    },
    MerchantBalanceMissing {
        merchant: MerchantId,
    },
    InsufficientMerchantCredit {
        merchant: MerchantId,
        requested: u64,
        available: u64,
    },
    ArithmeticOverflow,
}

impl fmt::Display for LoyaltyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMerchantId => write!(formatter, "merchant id must not be empty"),
            Self::EmptyCustomerId => write!(formatter, "customer id must not be empty"),
            Self::ZeroEarnBps => write!(formatter, "merchant earn rate must be positive"),
            Self::InvalidEarnBps { earn_bps } => {
                write!(formatter, "invalid earn rate: {earn_bps} bps")
            }
            Self::ZeroDailyCap => write!(formatter, "merchant daily cap must be positive"),
            Self::EmptyTierSchedule => write!(formatter, "tier schedule must have a threshold"),
            Self::ZeroTierThreshold => write!(formatter, "tier threshold must be positive"),
            Self::TooManyTiers { maximum } => {
                write!(formatter, "tier schedule exceeds maximum of {maximum}")
            }
            Self::NonIncreasingTierSchedule => {
                write!(formatter, "tier thresholds must be strictly increasing")
            }
            Self::ZeroMaxReceiptUnits => {
                write!(formatter, "maximum receipt contribution must be positive")
            }
            Self::MerchantInactive { merchant } => {
                write!(formatter, "merchant {} is inactive", merchant.as_str())
            }
            Self::ZeroReceipt => write!(formatter, "receipt amount must be positive"),
            Self::ZeroCredit => write!(formatter, "receipt earns zero credits"),
            Self::ZeroRedemption => write!(formatter, "redemption amount must be positive"),
            Self::NonceNotMonotonic { previous, received } => write!(
                formatter,
                "receipt nonce {received} is not greater than {previous}"
            ),
            Self::DailyCapExhausted { merchant, epoch } => write!(
                formatter,
                "merchant {} cap exhausted for epoch {epoch}",
                merchant.as_str()
            ),
            Self::MerchantBalanceMissing { merchant } => {
                write!(formatter, "no balance for merchant {}", merchant.as_str())
            }
            Self::InsufficientMerchantCredit {
                merchant,
                requested,
                available,
            } => write!(
                formatter,
                "merchant {} has {available} available; {requested} requested",
                merchant.as_str()
            ),
            Self::ArithmeticOverflow => write!(formatter, "loyalty arithmetic overflow"),
        }
    }
}

impl Error for LoyaltyError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn merchant(name: &str, bps: u16, cap: u64) -> Merchant {
        Merchant::new(MerchantId::new(name).unwrap(), bps, cap).unwrap()
    }

    fn rules() -> CoalitionRules {
        CoalitionRules::new(50, TierSchedule::new(vec![10, 30, 100]).unwrap()).unwrap()
    }

    #[test]
    fn accrual_is_capped_and_advances_tier() {
        let cafe = merchant("cafe", 1_000, 12);
        let mut passport = Passport::new("customer").unwrap();
        let event = passport.accrue(&cafe, 1, 200, 7, &rules()).unwrap();
        assert_eq!(event.credit_units, 12);
        assert_eq!(event.streak_delta, 12);
        assert_eq!(event.tier_level, 1);
        assert_eq!(passport.balance_for(cafe.id()).available_units(), 12);
    }

    #[test]
    fn same_or_lower_nonce_is_rejected_without_mutation() {
        let cafe = merchant("cafe", 1_000, 100);
        let mut passport = Passport::new("customer").unwrap();
        passport.accrue(&cafe, 4, 100, 1, &rules()).unwrap();
        let before = passport.clone();
        assert_eq!(
            passport.accrue(&cafe, 4, 100, 1, &rules()),
            Err(LoyaltyError::NonceNotMonotonic {
                previous: 4,
                received: 4
            })
        );
        assert_eq!(passport, before);
    }

    #[test]
    fn epoch_reset_preserves_credit_and_nonce_but_resets_cap_counter() {
        let cafe = merchant("cafe", 10_000, 10);
        let mut passport = Passport::new("customer").unwrap();
        passport.accrue(&cafe, 1, 10, 1, &rules()).unwrap();
        assert!(matches!(
            passport.accrue(&cafe, 2, 1, 1, &rules()),
            Err(LoyaltyError::DailyCapExhausted { .. })
        ));
        passport.accrue(&cafe, 2, 5, 2, &rules()).unwrap();
        let balance = passport.balance_for(cafe.id());
        assert_eq!(balance.earned_units, 15);
        assert_eq!(balance.earned_this_epoch, 5);
        assert_eq!(balance.last_receipt_nonce, 2);
    }

    #[test]
    fn merchant_balances_are_isolated() {
        let cafe = merchant("cafe", 10_000, 100);
        let bookstore = merchant("bookstore", 10_000, 100);
        let mut passport = Passport::new("customer").unwrap();
        passport.accrue(&cafe, 1, 20, 1, &rules()).unwrap();
        passport.accrue(&bookstore, 1, 40, 1, &rules()).unwrap();
        passport.redeem(cafe.id(), 10).unwrap();
        assert_eq!(passport.balance_for(cafe.id()).available_units(), 10);
        assert_eq!(passport.balance_for(bookstore.id()).available_units(), 40);
    }

    #[test]
    fn redemption_cannot_exceed_merchant_local_credit() {
        let cafe = merchant("cafe", 10_000, 100);
        let mut passport = Passport::new("customer").unwrap();
        passport.accrue(&cafe, 1, 20, 1, &rules()).unwrap();
        let before = passport.clone();
        assert!(matches!(
            passport.redeem(cafe.id(), 21),
            Err(LoyaltyError::InsufficientMerchantCredit { .. })
        ));
        assert_eq!(passport, before);
    }

    #[test]
    fn every_valid_small_receipt_preserves_balance_and_tier_invariants() {
        let cafe = merchant("cafe", 3_333, 10_000);
        let mut passport = Passport::new("customer").unwrap();
        for amount in 4..=500 {
            let event = passport
                .accrue(&cafe, amount, amount, amount, &rules())
                .unwrap();
            let balance = passport.balance_for(cafe.id());
            assert!(balance.earned_units >= balance.redeemed_units);
            assert!(event.streak_delta <= rules().max_receipt_units());
            assert_eq!(event.tier_level, passport.tier_level(&rules()));
        }
    }

    #[test]
    fn overflow_returns_error_without_state_change() {
        let cafe = merchant("cafe", 10_000, u64::MAX);
        let mut passport = Passport::new("customer").unwrap();
        let before = passport.clone();
        assert_eq!(
            passport.accrue(&cafe, 1, u64::MAX, 1, &rules()),
            Err(LoyaltyError::ArithmeticOverflow)
        );
        assert_eq!(passport, before);
    }

    #[test]
    fn failed_first_accrual_does_not_create_a_merchant_balance() {
        let cafe = merchant("cafe", 1_000, 100);
        let mut passport = Passport::new("customer").unwrap();
        let before = passport.clone();
        assert_eq!(
            passport.accrue(&cafe, 1, 1, 1, &rules()),
            Err(LoyaltyError::ZeroCredit)
        );
        assert_eq!(passport, before);
    }

    #[test]
    fn tier_schedule_rejects_ambiguous_thresholds() {
        assert_eq!(
            TierSchedule::new(vec![]),
            Err(LoyaltyError::EmptyTierSchedule)
        );
        assert_eq!(
            TierSchedule::new(vec![10, 10]),
            Err(LoyaltyError::NonIncreasingTierSchedule)
        );
        assert_eq!(
            TierSchedule::new(vec![0, 10]),
            Err(LoyaltyError::ZeroTierThreshold)
        );
        assert!(matches!(
            TierSchedule::new((1..=256).collect()),
            Err(LoyaltyError::TooManyTiers { .. })
        ));
    }

    #[test]
    fn constructors_reject_zero_effect_rules() {
        let merchant_id = MerchantId::new("cafe").unwrap();
        assert_eq!(
            Merchant::new(merchant_id.clone(), 0, 1),
            Err(LoyaltyError::ZeroEarnBps)
        );
        assert_eq!(
            Merchant::new(merchant_id, 1, 0),
            Err(LoyaltyError::ZeroDailyCap)
        );
        assert_eq!(
            CoalitionRules::new(0, TierSchedule::new(vec![1]).unwrap()),
            Err(LoyaltyError::ZeroMaxReceiptUnits)
        );
    }
}
