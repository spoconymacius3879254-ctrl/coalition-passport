//! Coalition Passport Anchor program.
//!
//! Coalition, merchant, customer Passport, and merchant-isolated balance PDAs
//! implement receipt-backed accrual and customer redemption. Each Passport is
//! paired with a one-of-one Token-2022 NonTransferable token.

// Anchor's generated BPF compatibility cfgs are not recognised by Rust 1.97's
// host-target cfg checker. Its generated abort branches are likewise known to
// Clippy as divergent expressions on the host target. These narrow allows are
// limited to framework expansion; all handwritten code still uses -D warnings.
#![allow(unexpected_cfgs)]
#![allow(clippy::diverging_sub_expression)]

use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};
use anchor_spl::{
    associated_token::{
        create as create_associated_token_account, get_associated_token_address_with_program_id,
        AssociatedToken, Create as CreateAssociatedToken,
    },
    token_2022::{
        initialize_mint2, mint_to_checked, set_authority,
        spl_token_2022::{extension::ExtensionType, instruction::AuthorityType, pod::PodMint},
        InitializeMint2, MintToChecked, SetAuthority, Token2022,
    },
    token_interface::{non_transferable_mint_initialize, NonTransferableMintInitialize},
};
use loyalty_core::BASIS_POINTS_DENOMINATOR;

declare_id!("2A2227YnW1PEr6FrMLxZrjm8B3P3fHWQjjqM8tDNhxg6");

/// Bound account allocation and prevent unbounded instruction data/state.
pub const MAX_TIERS: usize = 16;
const COALITION_SEED: &[u8] = b"coalition";
const MERCHANT_SEED: &[u8] = b"merchant";
const PASSPORT_SEED: &[u8] = b"passport";
const BALANCE_SEED: &[u8] = b"balance";
const SECONDS_PER_DAY: i64 = 86_400;
const STATE_VERSION: u8 = 1;

#[program]
pub mod coalition_passport {
    use super::*;

    /// Creates a coalition configuration PDA governed by `authority`.
    pub fn initialize_coalition(
        ctx: Context<InitializeCoalition>,
        max_receipt_units: u64,
        tier_thresholds: Vec<u64>,
    ) -> Result<()> {
        validate_coalition_config(max_receipt_units, &tier_thresholds)?;

        let coalition = &mut ctx.accounts.coalition;
        coalition.authority = ctx.accounts.authority.key();
        coalition.max_receipt_units = max_receipt_units;
        coalition.tier_count = u8::try_from(tier_thresholds.len())
            .map_err(|_| error!(CoalitionError::TooManyTiers))?;
        coalition.tier_thresholds = [0; MAX_TIERS];
        coalition.tier_thresholds[..tier_thresholds.len()].copy_from_slice(&tier_thresholds);
        coalition.paused = false;
        coalition.bump = ctx.bumps.coalition;

        emit!(CoalitionInitialized {
            coalition: coalition.key(),
            authority: coalition.authority,
            max_receipt_units,
            tier_count: coalition.tier_count,
        });
        Ok(())
    }

    /// Registers a merchant PDA after both coalition administrator and the
    /// merchant authority sign. Registration is blocked while paused.
    pub fn register_merchant(
        ctx: Context<RegisterMerchant>,
        earn_bps: u16,
        daily_cap: u64,
    ) -> Result<()> {
        validate_merchant_config(earn_bps, daily_cap)?;

        let merchant = &mut ctx.accounts.merchant;
        merchant.coalition = ctx.accounts.coalition.key();
        merchant.authority = ctx.accounts.merchant_authority.key();
        merchant.earn_bps = earn_bps;
        merchant.daily_cap = daily_cap;
        merchant.active = true;
        merchant.bump = ctx.bumps.merchant;

        emit!(MerchantRegistered {
            coalition: merchant.coalition,
            merchant: merchant.key(),
            merchant_authority: merchant.authority,
            earn_bps,
            daily_cap,
        });
        Ok(())
    }

    /// Halts coalition operations that opt into the coalition pause guard.
    /// Only the authority embedded in the coalition PDA may make this state
    /// transition.
    pub fn pause_coalition(ctx: Context<UpdateCoalitionPause>) -> Result<()> {
        let coalition = &mut ctx.accounts.coalition;
        set_coalition_paused(coalition, true)?;

        emit!(CoalitionPauseChanged {
            coalition: coalition.key(),
            authority: ctx.accounts.authority.key(),
            paused: true,
        });
        Ok(())
    }

    /// Resumes coalition operations after an authority-controlled pause.
    /// This intentionally uses the same PDA and authority checks as pause.
    pub fn unpause_coalition(ctx: Context<UpdateCoalitionPause>) -> Result<()> {
        let coalition = &mut ctx.accounts.coalition;
        set_coalition_paused(coalition, false)?;

        emit!(CoalitionPauseChanged {
            coalition: coalition.key(),
            authority: ctx.accounts.authority.key(),
            paused: false,
        });
        Ok(())
    }

    /// Creates the customer's unique Passport PDA and a one-of-one
    /// non-transferable Token-2022 credential. The mint authority is revoked
    /// after the single token is minted, permanently bounding supply at one.
    pub fn create_passport(ctx: Context<CreatePassport>) -> Result<()> {
        let mint_size =
            ExtensionType::try_calculate_account_len::<PodMint>(&[ExtensionType::NonTransferable])?;
        let mint_lamports = Rent::get()?.minimum_balance(mint_size);

        create_account(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                CreateAccount {
                    from: ctx.accounts.customer.to_account_info(),
                    to: ctx.accounts.passport_mint.to_account_info(),
                },
            ),
            mint_lamports,
            u64::try_from(mint_size).map_err(|_| error!(CoalitionError::MintSizeOverflow))?,
            &ctx.accounts.token_program.key(),
        )?;

        non_transferable_mint_initialize(CpiContext::new(
            ctx.accounts.token_program.key(),
            NonTransferableMintInitialize {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: ctx.accounts.passport_mint.to_account_info(),
            },
        ))?;

        let passport_key = ctx.accounts.passport.key();
        initialize_mint2(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                InitializeMint2 {
                    mint: ctx.accounts.passport_mint.to_account_info(),
                },
            ),
            0,
            &passport_key,
            None,
        )?;

        create_associated_token_account(CpiContext::new(
            ctx.accounts.associated_token_program.key(),
            CreateAssociatedToken {
                payer: ctx.accounts.customer.to_account_info(),
                associated_token: ctx.accounts.passport_token.to_account_info(),
                authority: ctx.accounts.customer.to_account_info(),
                mint: ctx.accounts.passport_mint.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
            },
        ))?;

        let coalition_key = ctx.accounts.coalition.key();
        let customer_key = ctx.accounts.customer.key();
        let passport_bump = ctx.bumps.passport;
        let signer_seeds: &[&[u8]] = &[
            PASSPORT_SEED,
            coalition_key.as_ref(),
            customer_key.as_ref(),
            &[passport_bump],
        ];
        let signer = &[signer_seeds];

        mint_to_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                MintToChecked {
                    mint: ctx.accounts.passport_mint.to_account_info(),
                    to: ctx.accounts.passport_token.to_account_info(),
                    authority: ctx.accounts.passport.to_account_info(),
                },
                signer,
            ),
            1,
            0,
        )?;

        set_authority(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                SetAuthority {
                    current_authority: ctx.accounts.passport.to_account_info(),
                    account_or_mint: ctx.accounts.passport_mint.to_account_info(),
                },
                signer,
            ),
            AuthorityType::MintTokens,
            None,
        )?;

        let passport = &mut ctx.accounts.passport;
        passport.coalition = coalition_key;
        passport.customer = customer_key;
        passport.passport_mint = ctx.accounts.passport_mint.key();
        passport.total_visits = 0;
        passport.streak_points = 0;
        passport.bump = passport_bump;
        passport.version = 1;

        emit!(PassportCreated {
            coalition: coalition_key,
            passport: passport.key(),
            customer: customer_key,
            passport_mint: passport.passport_mint,
            passport_token: ctx.accounts.passport_token.key(),
        });
        Ok(())
    }

    /// Records one merchant-authenticated receipt into the customer's
    /// merchant-isolated balance. The trusted Clock sysvar supplies the daily
    /// cap epoch; callers cannot reset caps by choosing an epoch value.
    pub fn record_receipt(
        ctx: Context<RecordReceipt>,
        nonce: u64,
        amount_units: u64,
        receipt_hash: [u8; 32],
    ) -> Result<()> {
        require!(receipt_hash != [0; 32], CoalitionError::EmptyReceiptHash);
        let epoch = current_day_epoch()?;
        let coalition_key = ctx.accounts.coalition.key();
        let merchant_key = ctx.accounts.merchant.key();
        let passport_key = ctx.accounts.passport.key();
        let balance_bump = ctx.bumps.balance;

        let outcome = apply_receipt(
            &ctx.accounts.coalition,
            &ctx.accounts.merchant,
            &mut ctx.accounts.passport,
            &mut ctx.accounts.balance,
            passport_key,
            merchant_key,
            balance_bump,
            nonce,
            amount_units,
            epoch,
        )?;

        emit!(ReceiptRecorded {
            coalition: coalition_key,
            passport: passport_key,
            merchant: merchant_key,
            nonce,
            epoch,
            credit_units: outcome.credit_units,
            streak_delta: outcome.streak_delta,
            tier_level: outcome.tier_level,
            receipt_hash,
        });
        Ok(())
    }

    /// Redeems only the signing customer's credits for one explicit merchant.
    /// Coalition pause intentionally does not block redemption of existing
    /// credit, preventing an administrator from trapping customer balances.
    pub fn redeem(ctx: Context<Redeem>, units: u64) -> Result<()> {
        let remaining_units = apply_redemption(&mut ctx.accounts.balance, units)?;

        emit!(Redeemed {
            coalition: ctx.accounts.coalition.key(),
            passport: ctx.accounts.passport.key(),
            merchant: ctx.accounts.merchant.key(),
            customer: ctx.accounts.customer.key(),
            redeemed_units: units,
            remaining_units,
        });
        Ok(())
    }
}

/// Coalition PDA state. Tier thresholds are fixed-width to make account size
/// deterministic and to reject oversized initialization instructions.
#[account]
#[derive(InitSpace)]
pub struct Coalition {
    pub authority: Pubkey,
    pub max_receipt_units: u64,
    pub tier_count: u8,
    pub tier_thresholds: [u64; MAX_TIERS],
    pub paused: bool,
    pub bump: u8,
}

/// Merchant PDA state. Credits will remain merchant-local in a later stage.
#[account]
#[derive(InitSpace)]
pub struct Merchant {
    pub coalition: Pubkey,
    pub authority: Pubkey,
    pub earn_bps: u16,
    pub daily_cap: u64,
    pub active: bool,
    pub bump: u8,
}

/// Customer-owned coalition reputation. Spendable credits are deliberately
/// kept out of this account and will live in merchant-isolated balance PDAs.
#[account]
#[derive(InitSpace)]
pub struct Passport {
    pub coalition: Pubkey,
    pub customer: Pubkey,
    pub passport_mint: Pubkey,
    pub total_visits: u64,
    pub streak_points: u64,
    pub bump: u8,
    pub version: u8,
}

/// One customer's credit ledger for exactly one merchant. PDA seeds bind the
/// account to both sides, so a merchant signer cannot redirect an accrual into
/// another merchant's liability bucket.
#[account]
#[derive(InitSpace)]
pub struct MerchantBalance {
    pub passport: Pubkey,
    pub merchant: Pubkey,
    pub earned_units: u64,
    pub redeemed_units: u64,
    pub earned_this_epoch: u64,
    pub cap_epoch: u64,
    pub last_receipt_nonce: u64,
    pub bump: u8,
    pub version: u8,
}

#[derive(Accounts)]
pub struct InitializeCoalition<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + Coalition::INIT_SPACE,
        seeds = [COALITION_SEED, authority.key().as_ref()],
        bump
    )]
    pub coalition: Account<'info, Coalition>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RegisterMerchant<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        seeds = [COALITION_SEED, authority.key().as_ref()],
        bump = coalition.bump,
        has_one = authority @ CoalitionError::UnauthorizedAuthority,
        constraint = !coalition.paused @ CoalitionError::CoalitionPaused
    )]
    pub coalition: Account<'info, Coalition>,
    /// The merchant has to consent to controlling the registered merchant PDA.
    pub merchant_authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + Merchant::INIT_SPACE,
        seeds = [MERCHANT_SEED, coalition.key().as_ref(), merchant_authority.key().as_ref()],
        bump
    )]
    pub merchant: Account<'info, Merchant>,
    pub system_program: Program<'info, System>,
}

/// Account validation shared by the authority-only pause transitions. The
/// seed constraint binds the address to the signing authority and `has_one`
/// verifies that the coalition data names that same authority.
#[derive(Accounts)]
pub struct UpdateCoalitionPause<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [COALITION_SEED, authority.key().as_ref()],
        bump = coalition.bump,
        has_one = authority @ CoalitionError::UnauthorizedAuthority
    )]
    pub coalition: Account<'info, Coalition>,
}

#[derive(Accounts)]
pub struct CreatePassport<'info> {
    #[account(mut)]
    pub customer: Signer<'info>,
    #[account(
        seeds = [COALITION_SEED, coalition.authority.as_ref()],
        bump = coalition.bump,
        constraint = !coalition.paused @ CoalitionError::CoalitionPaused
    )]
    pub coalition: Account<'info, Coalition>,
    #[account(
        init,
        payer = customer,
        space = 8 + Passport::INIT_SPACE,
        seeds = [PASSPORT_SEED, coalition.key().as_ref(), customer.key().as_ref()],
        bump
    )]
    pub passport: Account<'info, Passport>,
    /// The customer supplies a fresh mint signer. The unique Passport PDA
    /// prevents a second mint for the same coalition/customer pair.
    #[account(mut)]
    pub passport_mint: Signer<'info>,
    /// CHECK: constrained to the canonical Token-2022 ATA for customer/mint;
    /// the associated-token program initializes and validates its contents.
    #[account(
        mut,
        address = get_associated_token_address_with_program_id(
            &customer.key(),
            &passport_mint.key(),
            &token_program.key()
        ) @ CoalitionError::InvalidPassportTokenAccount
    )]
    pub passport_token: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RecordReceipt<'info> {
    #[account(mut)]
    pub merchant_authority: Signer<'info>,
    #[account(
        seeds = [COALITION_SEED, coalition.authority.as_ref()],
        bump = coalition.bump,
        constraint = !coalition.paused @ CoalitionError::CoalitionPaused
    )]
    pub coalition: Account<'info, Coalition>,
    #[account(
        seeds = [MERCHANT_SEED, coalition.key().as_ref(), merchant_authority.key().as_ref()],
        bump = merchant.bump,
        has_one = coalition @ CoalitionError::AccountRelationshipMismatch,
        constraint = merchant.authority == merchant_authority.key()
            @ CoalitionError::UnauthorizedMerchant,
        constraint = merchant.active @ CoalitionError::MerchantInactive
    )]
    pub merchant: Account<'info, Merchant>,
    #[account(
        mut,
        seeds = [PASSPORT_SEED, coalition.key().as_ref(), passport.customer.as_ref()],
        bump = passport.bump,
        has_one = coalition @ CoalitionError::AccountRelationshipMismatch
    )]
    pub passport: Account<'info, Passport>,
    #[account(
        init_if_needed,
        payer = merchant_authority,
        space = 8 + MerchantBalance::INIT_SPACE,
        seeds = [BALANCE_SEED, passport.key().as_ref(), merchant.key().as_ref()],
        bump
    )]
    pub balance: Account<'info, MerchantBalance>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Redeem<'info> {
    pub customer: Signer<'info>,
    #[account(
        seeds = [COALITION_SEED, coalition.authority.as_ref()],
        bump = coalition.bump
    )]
    pub coalition: Account<'info, Coalition>,
    #[account(
        seeds = [PASSPORT_SEED, coalition.key().as_ref(), customer.key().as_ref()],
        bump = passport.bump,
        has_one = coalition @ CoalitionError::AccountRelationshipMismatch,
        has_one = customer @ CoalitionError::UnauthorizedCustomer
    )]
    pub passport: Account<'info, Passport>,
    #[account(
        seeds = [MERCHANT_SEED, coalition.key().as_ref(), merchant.authority.as_ref()],
        bump = merchant.bump,
        has_one = coalition @ CoalitionError::AccountRelationshipMismatch
    )]
    pub merchant: Account<'info, Merchant>,
    #[account(
        mut,
        seeds = [BALANCE_SEED, passport.key().as_ref(), merchant.key().as_ref()],
        bump = balance.bump,
        has_one = passport @ CoalitionError::AccountRelationshipMismatch,
        has_one = merchant @ CoalitionError::AccountRelationshipMismatch
    )]
    pub balance: Account<'info, MerchantBalance>,
}

#[event]
pub struct CoalitionInitialized {
    pub coalition: Pubkey,
    pub authority: Pubkey,
    pub max_receipt_units: u64,
    pub tier_count: u8,
}

#[event]
pub struct MerchantRegistered {
    pub coalition: Pubkey,
    pub merchant: Pubkey,
    pub merchant_authority: Pubkey,
    pub earn_bps: u16,
    pub daily_cap: u64,
}

#[event]
pub struct CoalitionPauseChanged {
    pub coalition: Pubkey,
    pub authority: Pubkey,
    pub paused: bool,
}

#[event]
pub struct PassportCreated {
    pub coalition: Pubkey,
    pub passport: Pubkey,
    pub customer: Pubkey,
    pub passport_mint: Pubkey,
    pub passport_token: Pubkey,
}

#[event]
pub struct ReceiptRecorded {
    pub coalition: Pubkey,
    pub passport: Pubkey,
    pub merchant: Pubkey,
    pub nonce: u64,
    pub epoch: u64,
    pub credit_units: u64,
    pub streak_delta: u64,
    pub tier_level: u8,
    pub receipt_hash: [u8; 32],
}

#[event]
pub struct Redeemed {
    pub coalition: Pubkey,
    pub passport: Pubkey,
    pub merchant: Pubkey,
    pub customer: Pubkey,
    pub redeemed_units: u64,
    pub remaining_units: u64,
}

/// Validates configuration before any account data is persisted.
pub fn validate_coalition_config(max_receipt_units: u64, tiers: &[u64]) -> Result<()> {
    require!(max_receipt_units > 0, CoalitionError::ZeroMaxReceiptUnits);
    require!(!tiers.is_empty(), CoalitionError::EmptyTierSchedule);
    require!(tiers.len() <= MAX_TIERS, CoalitionError::TooManyTiers);
    require!(tiers[0] > 0, CoalitionError::ZeroTierThreshold);
    for pair in tiers.windows(2) {
        require!(pair[0] < pair[1], CoalitionError::NonIncreasingTierSchedule);
    }
    Ok(())
}

/// Uses the Stage 0 basis-point denominator so merchant rewards follow the
/// same rate boundary in both the off-chain core and future on-chain accrual.
pub fn validate_merchant_config(earn_bps: u16, daily_cap: u64) -> Result<()> {
    require!(earn_bps > 0, CoalitionError::ZeroEarnBps);
    require!(
        u64::from(earn_bps) <= BASIS_POINTS_DENOMINATOR,
        CoalitionError::InvalidEarnBps
    );
    require!(daily_cap > 0, CoalitionError::ZeroDailyCap);
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
pub struct ReceiptOutcome {
    pub credit_units: u64,
    pub streak_delta: u64,
    pub tier_level: u8,
}

/// Converts the validator-provided Unix timestamp into a monotonic day bucket.
/// A negative timestamp is rejected instead of wrapping into a large `u64`.
pub fn current_day_epoch() -> Result<u64> {
    let timestamp = Clock::get()?.unix_timestamp;
    let day = timestamp.div_euclid(SECONDS_PER_DAY);
    u64::try_from(day).map_err(|_| error!(CoalitionError::InvalidClock))
}

/// Applies a receipt only after every relationship, cap, replay, and checked
/// arithmetic condition has succeeded. This preserves transaction atomicity
/// even when the balance is being initialized by `init_if_needed`.
#[allow(clippy::too_many_arguments)]
pub fn apply_receipt(
    coalition: &Coalition,
    merchant: &Merchant,
    passport: &mut Passport,
    balance: &mut MerchantBalance,
    passport_key: Pubkey,
    merchant_key: Pubkey,
    balance_bump: u8,
    nonce: u64,
    amount_units: u64,
    epoch: u64,
) -> Result<ReceiptOutcome> {
    require!(merchant.active, CoalitionError::MerchantInactive);
    require!(amount_units > 0, CoalitionError::ZeroReceipt);
    require!(
        passport.coalition == merchant.coalition,
        CoalitionError::AccountRelationshipMismatch
    );

    let is_new = balance.version == 0;
    if !is_new {
        require!(
            balance.version == STATE_VERSION,
            CoalitionError::UnsupportedStateVersion
        );
        require!(
            balance.passport == passport_key && balance.merchant == merchant_key,
            CoalitionError::AccountRelationshipMismatch
        );
        require!(balance.bump == balance_bump, CoalitionError::CorruptBalance);
        require!(
            epoch >= balance.cap_epoch,
            CoalitionError::ClockMovedBackwards
        );
    }
    require!(
        nonce > balance.last_receipt_nonce,
        CoalitionError::NonceNotMonotonic
    );
    require!(
        balance.earned_units >= balance.redeemed_units,
        CoalitionError::CorruptBalance
    );

    let raw_credit = amount_units
        .checked_mul(u64::from(merchant.earn_bps))
        .ok_or_else(|| error!(CoalitionError::ArithmeticOverflow))?
        .checked_div(BASIS_POINTS_DENOMINATOR)
        .ok_or_else(|| error!(CoalitionError::ArithmeticOverflow))?;
    require!(raw_credit > 0, CoalitionError::ZeroCredit);

    let earned_this_epoch = if !is_new && balance.cap_epoch == epoch {
        balance.earned_this_epoch
    } else {
        0
    };
    let cap_remaining = merchant
        .daily_cap
        .checked_sub(earned_this_epoch)
        .ok_or_else(|| error!(CoalitionError::CorruptBalance))?;
    require!(cap_remaining > 0, CoalitionError::DailyCapExhausted);
    let credit_units = raw_credit.min(cap_remaining);
    let streak_delta = credit_units.min(coalition.max_receipt_units);

    let new_earned_units = balance
        .earned_units
        .checked_add(credit_units)
        .ok_or_else(|| error!(CoalitionError::ArithmeticOverflow))?;
    let new_earned_this_epoch = earned_this_epoch
        .checked_add(credit_units)
        .ok_or_else(|| error!(CoalitionError::ArithmeticOverflow))?;
    let new_total_visits = passport
        .total_visits
        .checked_add(1)
        .ok_or_else(|| error!(CoalitionError::ArithmeticOverflow))?;
    let new_streak_points = passport
        .streak_points
        .checked_add(streak_delta)
        .ok_or_else(|| error!(CoalitionError::ArithmeticOverflow))?;
    let tier_level = tier_level_for(coalition, new_streak_points);

    balance.passport = passport_key;
    balance.merchant = merchant_key;
    balance.earned_units = new_earned_units;
    balance.earned_this_epoch = new_earned_this_epoch;
    balance.cap_epoch = epoch;
    balance.last_receipt_nonce = nonce;
    balance.bump = balance_bump;
    balance.version = STATE_VERSION;
    passport.total_visits = new_total_visits;
    passport.streak_points = new_streak_points;

    Ok(ReceiptOutcome {
        credit_units,
        streak_delta,
        tier_level,
    })
}

/// Applies a customer-authorized redemption to one merchant-local balance.
pub fn apply_redemption(balance: &mut MerchantBalance, units: u64) -> Result<u64> {
    require!(units > 0, CoalitionError::ZeroRedemption);
    require!(
        balance.version == STATE_VERSION,
        CoalitionError::UnsupportedStateVersion
    );
    let available = balance
        .earned_units
        .checked_sub(balance.redeemed_units)
        .ok_or_else(|| error!(CoalitionError::CorruptBalance))?;
    require!(
        units <= available,
        CoalitionError::InsufficientMerchantCredit
    );
    let new_redeemed = balance
        .redeemed_units
        .checked_add(units)
        .ok_or_else(|| error!(CoalitionError::ArithmeticOverflow))?;
    let remaining = balance
        .earned_units
        .checked_sub(new_redeemed)
        .ok_or_else(|| error!(CoalitionError::CorruptBalance))?;
    balance.redeemed_units = new_redeemed;
    Ok(remaining)
}

#[must_use]
pub fn tier_level_for(coalition: &Coalition, streak_points: u64) -> u8 {
    coalition.tier_thresholds[..usize::from(coalition.tier_count)]
        .iter()
        .take_while(|&&threshold| streak_points >= threshold)
        .count()
        .try_into()
        .unwrap_or(u8::MAX)
}

/// Applies one non-idempotent pause transition. Keeping this separate from
/// the instruction handlers makes the transition rules deterministic and
/// directly testable without an RPC runtime.
pub fn set_coalition_paused(coalition: &mut Coalition, paused: bool) -> Result<()> {
    match (coalition.paused, paused) {
        (true, true) => err!(CoalitionError::AlreadyPaused),
        (false, false) => err!(CoalitionError::AlreadyUnpaused),
        _ => {
            coalition.paused = paused;
            Ok(())
        }
    }
}

#[error_code]
pub enum CoalitionError {
    #[msg("the coalition authority did not sign")]
    UnauthorizedAuthority,
    #[msg("the coalition is paused")]
    CoalitionPaused,
    #[msg("the coalition is already paused")]
    AlreadyPaused,
    #[msg("the coalition is already unpaused")]
    AlreadyUnpaused,
    #[msg("maximum receipt units must be greater than zero")]
    ZeroMaxReceiptUnits,
    #[msg("at least one tier threshold is required")]
    EmptyTierSchedule,
    #[msg("tier threshold count exceeds the fixed account capacity")]
    TooManyTiers,
    #[msg("tier thresholds must be greater than zero")]
    ZeroTierThreshold,
    #[msg("tier thresholds must be strictly increasing")]
    NonIncreasingTierSchedule,
    #[msg("merchant earn basis points must be greater than zero")]
    ZeroEarnBps,
    #[msg("merchant earn basis points exceed 100 percent")]
    InvalidEarnBps,
    #[msg("merchant daily cap must be greater than zero")]
    ZeroDailyCap,
    #[msg("the passport mint account size cannot be represented safely")]
    MintSizeOverflow,
    #[msg("the supplied passport token account is not the canonical Token-2022 ATA")]
    InvalidPassportTokenAccount,
    #[msg("the merchant authority did not authorize this merchant PDA")]
    UnauthorizedMerchant,
    #[msg("the customer did not authorize this Passport")]
    UnauthorizedCustomer,
    #[msg("the supplied accounts do not belong to the same coalition relationship")]
    AccountRelationshipMismatch,
    #[msg("the merchant is inactive")]
    MerchantInactive,
    #[msg("receipt hash must be a nonzero opaque commitment")]
    EmptyReceiptHash,
    #[msg("receipt amount must be greater than zero")]
    ZeroReceipt,
    #[msg("receipt amount earns zero credit at this merchant rate")]
    ZeroCredit,
    #[msg("receipt nonce must strictly increase for this merchant balance")]
    NonceNotMonotonic,
    #[msg("the merchant daily earning cap is exhausted")]
    DailyCapExhausted,
    #[msg("loyalty arithmetic overflowed")]
    ArithmeticOverflow,
    #[msg("validator clock cannot be converted into a nonnegative day epoch")]
    InvalidClock,
    #[msg("validator day epoch moved backwards for this balance")]
    ClockMovedBackwards,
    #[msg("account uses an unsupported state version")]
    UnsupportedStateVersion,
    #[msg("merchant balance invariants are corrupt")]
    CorruptBalance,
    #[msg("redemption units must be greater than zero")]
    ZeroRedemption,
    #[msg("redemption exceeds this merchant's available customer credit")]
    InsufficientMerchantCredit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalition_validation_accepts_bounded_strict_tiers() {
        assert!(validate_coalition_config(50, &[10, 25, 100]).is_ok());
    }

    #[test]
    fn coalition_validation_rejects_zero_and_ambiguous_values() {
        assert!(validate_coalition_config(0, &[1]).is_err());
        assert!(validate_coalition_config(1, &[]).is_err());
        assert!(validate_coalition_config(1, &[0]).is_err());
        assert!(validate_coalition_config(1, &[10, 10]).is_err());
        assert!(validate_coalition_config(1, &[11, 10]).is_err());
    }

    #[test]
    fn merchant_validation_matches_stage_zero_basis_point_limits() {
        assert!(validate_merchant_config(1, 1).is_ok());
        assert!(validate_merchant_config(10_000, 1).is_ok());
        assert!(validate_merchant_config(0, 1).is_err());
        assert!(validate_merchant_config(10_001, 1).is_err());
        assert!(validate_merchant_config(1, 0).is_err());
    }

    #[test]
    fn pause_transition_changes_state_once() {
        let mut coalition = test_coalition(false);

        assert!(set_coalition_paused(&mut coalition, true).is_ok());
        assert!(coalition.paused);
        assert!(set_coalition_paused(&mut coalition, true).is_err());
        assert!(coalition.paused);
    }

    #[test]
    fn unpause_transition_changes_state_once() {
        let mut coalition = test_coalition(true);

        assert!(set_coalition_paused(&mut coalition, false).is_ok());
        assert!(!coalition.paused);
        assert!(set_coalition_paused(&mut coalition, false).is_err());
        assert!(!coalition.paused);
    }

    #[test]
    fn receipt_cap_replay_epoch_and_redemption_are_atomic() {
        let coalition_key = Pubkey::new_unique();
        let passport_key = Pubkey::new_unique();
        let merchant_key = Pubkey::new_unique();
        let coalition = configured_coalition();
        let merchant = Merchant {
            coalition: coalition_key,
            authority: Pubkey::new_unique(),
            earn_bps: 1_000,
            daily_cap: 12,
            active: true,
            bump: 7,
        };
        let mut passport = Passport {
            coalition: coalition_key,
            customer: Pubkey::new_unique(),
            passport_mint: Pubkey::new_unique(),
            total_visits: 0,
            streak_points: 0,
            bump: 9,
            version: STATE_VERSION,
        };
        let mut balance = empty_balance();

        let first = apply_receipt(
            &coalition,
            &merchant,
            &mut passport,
            &mut balance,
            passport_key,
            merchant_key,
            4,
            1,
            200,
            7,
        )
        .unwrap();
        assert_eq!(
            first,
            ReceiptOutcome {
                credit_units: 12,
                streak_delta: 12,
                tier_level: 1,
            }
        );
        assert_eq!(balance.earned_units, 12);
        assert_eq!(balance.earned_this_epoch, 12);
        assert_eq!(passport.total_visits, 1);

        let before = balance_snapshot(&balance, &passport);
        assert!(apply_receipt(
            &coalition,
            &merchant,
            &mut passport,
            &mut balance,
            passport_key,
            merchant_key,
            4,
            1,
            100,
            7,
        )
        .is_err());
        assert_eq!(balance_snapshot(&balance, &passport), before);

        assert!(apply_receipt(
            &coalition,
            &merchant,
            &mut passport,
            &mut balance,
            passport_key,
            merchant_key,
            4,
            2,
            100,
            7,
        )
        .is_err());
        assert_eq!(balance_snapshot(&balance, &passport), before);

        let next_day = apply_receipt(
            &coalition,
            &merchant,
            &mut passport,
            &mut balance,
            passport_key,
            merchant_key,
            4,
            2,
            100,
            8,
        )
        .unwrap();
        assert_eq!(next_day.credit_units, 10);
        assert_eq!(balance.earned_units, 22);
        assert_eq!(balance.earned_this_epoch, 10);
        let before_clock_regression = balance_snapshot(&balance, &passport);
        assert!(apply_receipt(
            &coalition,
            &merchant,
            &mut passport,
            &mut balance,
            passport_key,
            merchant_key,
            4,
            3,
            100,
            7,
        )
        .is_err());
        assert_eq!(
            balance_snapshot(&balance, &passport),
            before_clock_regression
        );

        assert_eq!(apply_redemption(&mut balance, 5).unwrap(), 17);
        let redeemed_before_error = balance.redeemed_units;
        assert!(apply_redemption(&mut balance, 18).is_err());
        assert_eq!(balance.redeemed_units, redeemed_before_error);
    }

    #[test]
    fn failed_first_receipt_leaves_new_balance_uninitialized() {
        let coalition_key = Pubkey::new_unique();
        let passport_key = Pubkey::new_unique();
        let merchant_key = Pubkey::new_unique();
        let coalition = configured_coalition();
        let merchant = Merchant {
            coalition: coalition_key,
            authority: Pubkey::new_unique(),
            earn_bps: 1,
            daily_cap: 10,
            active: true,
            bump: 1,
        };
        let mut passport = Passport {
            coalition: coalition_key,
            customer: Pubkey::new_unique(),
            passport_mint: Pubkey::new_unique(),
            total_visits: 0,
            streak_points: 0,
            bump: 2,
            version: STATE_VERSION,
        };
        let mut balance = empty_balance();

        assert!(apply_receipt(
            &coalition,
            &merchant,
            &mut passport,
            &mut balance,
            passport_key,
            merchant_key,
            3,
            1,
            1,
            1,
        )
        .is_err());
        assert_eq!(balance.version, 0);
        assert_eq!(balance.passport, Pubkey::default());
        assert_eq!(passport.total_visits, 0);
        assert_eq!(passport.streak_points, 0);
    }

    fn test_coalition(paused: bool) -> Coalition {
        Coalition {
            authority: Pubkey::new_unique(),
            max_receipt_units: 1,
            tier_count: 1,
            tier_thresholds: [1; MAX_TIERS],
            paused,
            bump: 0,
        }
    }

    fn configured_coalition() -> Coalition {
        let mut tiers = [0; MAX_TIERS];
        tiers[..3].copy_from_slice(&[10, 25, 100]);
        Coalition {
            authority: Pubkey::new_unique(),
            max_receipt_units: 50,
            tier_count: 3,
            tier_thresholds: tiers,
            paused: false,
            bump: 0,
        }
    }

    fn empty_balance() -> MerchantBalance {
        MerchantBalance {
            passport: Pubkey::default(),
            merchant: Pubkey::default(),
            earned_units: 0,
            redeemed_units: 0,
            earned_this_epoch: 0,
            cap_epoch: 0,
            last_receipt_nonce: 0,
            bump: 0,
            version: 0,
        }
    }

    fn balance_snapshot(
        balance: &MerchantBalance,
        passport: &Passport,
    ) -> (Pubkey, Pubkey, u64, u64, u64, u64, u64, u8, u8, u64, u64) {
        (
            balance.passport,
            balance.merchant,
            balance.earned_units,
            balance.redeemed_units,
            balance.earned_this_epoch,
            balance.cap_epoch,
            balance.last_receipt_nonce,
            balance.bump,
            balance.version,
            passport.total_visits,
            passport.streak_points,
        )
    }
}
