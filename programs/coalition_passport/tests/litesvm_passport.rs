use std::path::Path;

use anchor_lang::{prelude::Pubkey, AccountDeserialize, InstructionData};
use anchor_spl::{
    associated_token::{get_associated_token_address_with_program_id, ID as ASSOCIATED_TOKEN_ID},
    token_2022::{
        spl_token_2022::{
            extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions},
            instruction as token_instruction,
            state::{Account as TokenAccount, Mint},
        },
        ID as TOKEN_2022_ID,
    },
};
use coalition_passport::{instruction, Coalition, Merchant, MerchantBalance, Passport, ID};
use litesvm::LiteSVM;
use solana_address::Address;
use solana_clock::Clock;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_system_interface::program::ID as SYSTEM_PROGRAM_ID;
use solana_transaction::Transaction;

fn send(
    svm: &mut LiteSVM,
    payer: &Keypair,
    additional_signers: &[&Keypair],
    instruction: Instruction,
) -> Result<litesvm::types::TransactionMetadata, Box<litesvm::types::FailedTransactionMetadata>> {
    let mut signers = vec![payer];
    signers.extend_from_slice(additional_signers);
    let transaction = Transaction::new(
        &signers,
        Message::new(&[instruction], Some(&payer.pubkey())),
        svm.latest_blockhash(),
    );
    svm.send_transaction(transaction).map_err(Box::new)
}

fn address(pubkey: Pubkey) -> Address {
    Address::new_from_array(pubkey.to_bytes())
}

fn pubkey(address: Address) -> Pubkey {
    Pubkey::new_from_array(address.to_bytes())
}

#[allow(clippy::too_many_arguments)]
fn record_receipt_instruction(
    program_id: Address,
    merchant_authority: Address,
    coalition: Pubkey,
    merchant: Pubkey,
    passport: Pubkey,
    balance: Pubkey,
    nonce: u64,
    amount_units: u64,
    receipt_hash: [u8; 32],
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(merchant_authority, true),
            AccountMeta::new_readonly(address(coalition), false),
            AccountMeta::new_readonly(address(merchant), false),
            AccountMeta::new(address(passport), false),
            AccountMeta::new(address(balance), false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: instruction::RecordReceipt {
            nonce,
            amount_units,
            receipt_hash,
        }
        .data(),
    }
}

fn redeem_instruction(
    program_id: Address,
    customer: Address,
    coalition: Pubkey,
    passport: Pubkey,
    merchant: Pubkey,
    balance: Pubkey,
    units: u64,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new_readonly(customer, true),
            AccountMeta::new_readonly(address(coalition), false),
            AccountMeta::new_readonly(address(passport), false),
            AccountMeta::new_readonly(address(merchant), false),
            AccountMeta::new(address(balance), false),
        ],
        data: instruction::Redeem { units }.data(),
    }
}

#[test]
fn creates_one_non_transferable_passport_with_revoked_mint_authority() {
    let mut svm = LiteSVM::new().with_default_programs();
    let program_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/coalition_passport.so");
    let program_id = address(ID);
    svm.add_program_from_file(program_id, program_path).unwrap();

    let customer = Keypair::new();
    svm.airdrop(&customer.pubkey(), 10_000_000_000).unwrap();

    let (coalition, _) =
        Pubkey::find_program_address(&[b"coalition", customer.pubkey().as_ref()], &ID);
    let initialize = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(customer.pubkey(), true),
            AccountMeta::new(address(coalition), false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: instruction::InitializeCoalition {
            max_receipt_units: 50,
            tier_thresholds: vec![10, 25, 100],
        }
        .data(),
    };
    send(&mut svm, &customer, &[], initialize).unwrap();

    let coalition_account = svm.get_account(&address(coalition)).unwrap();
    let coalition_state =
        Coalition::try_deserialize(&mut coalition_account.data.as_slice()).unwrap();
    assert_eq!(coalition_state.authority, pubkey(customer.pubkey()));
    assert!(!coalition_state.paused);

    let pause = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new_readonly(customer.pubkey(), true),
            AccountMeta::new(address(coalition), false),
        ],
        data: instruction::PauseCoalition {}.data(),
    };
    send(&mut svm, &customer, &[], pause).unwrap();

    let attacker = Keypair::new();
    let unauthorized_unpause = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new_readonly(attacker.pubkey(), true),
            AccountMeta::new(address(coalition), false),
        ],
        data: instruction::UnpauseCoalition {}.data(),
    };
    assert!(send(&mut svm, &customer, &[&attacker], unauthorized_unpause).is_err());
    let coalition_account = svm.get_account(&address(coalition)).unwrap();
    let coalition_state =
        Coalition::try_deserialize(&mut coalition_account.data.as_slice()).unwrap();
    assert!(coalition_state.paused);

    let paused_mint = Keypair::new();
    let (passport, _) = Pubkey::find_program_address(
        &[b"passport", coalition.as_ref(), customer.pubkey().as_ref()],
        &ID,
    );
    let paused_token = get_associated_token_address_with_program_id(
        &pubkey(customer.pubkey()),
        &pubkey(paused_mint.pubkey()),
        &TOKEN_2022_ID,
    );
    let paused_create = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(customer.pubkey(), true),
            AccountMeta::new_readonly(address(coalition), false),
            AccountMeta::new(address(passport), false),
            AccountMeta::new(paused_mint.pubkey(), true),
            AccountMeta::new(address(paused_token), false),
            AccountMeta::new_readonly(address(TOKEN_2022_ID), false),
            AccountMeta::new_readonly(address(ASSOCIATED_TOKEN_ID), false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: instruction::CreatePassport {}.data(),
    };
    assert!(send(&mut svm, &customer, &[&paused_mint], paused_create).is_err());
    assert!(svm.get_account(&paused_mint.pubkey()).is_none());

    let unpause = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new_readonly(customer.pubkey(), true),
            AccountMeta::new(address(coalition), false),
        ],
        data: instruction::UnpauseCoalition {}.data(),
    };
    send(&mut svm, &customer, &[], unpause).unwrap();

    let merchant_authority = Keypair::new();
    svm.airdrop(&merchant_authority.pubkey(), 2_000_000_000)
        .unwrap();
    let (merchant, _) = Pubkey::find_program_address(
        &[
            b"merchant",
            coalition.as_ref(),
            merchant_authority.pubkey().as_ref(),
        ],
        &ID,
    );
    let register_merchant = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(customer.pubkey(), true),
            AccountMeta::new_readonly(address(coalition), false),
            AccountMeta::new_readonly(merchant_authority.pubkey(), true),
            AccountMeta::new(address(merchant), false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: instruction::RegisterMerchant {
            earn_bps: 1_000,
            daily_cap: 12,
        }
        .data(),
    };
    send(
        &mut svm,
        &customer,
        &[&merchant_authority],
        register_merchant,
    )
    .unwrap();
    let merchant_account = svm.get_account(&address(merchant)).unwrap();
    let merchant_state = Merchant::try_deserialize(&mut merchant_account.data.as_slice()).unwrap();
    assert_eq!(
        merchant_state.authority,
        pubkey(merchant_authority.pubkey())
    );
    assert!(merchant_state.active);

    let mint = Keypair::new();
    let passport_token = get_associated_token_address_with_program_id(
        &pubkey(customer.pubkey()),
        &mint.pubkey(),
        &TOKEN_2022_ID,
    );
    let create_passport = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(customer.pubkey(), true),
            AccountMeta::new_readonly(address(coalition), false),
            AccountMeta::new(address(passport), false),
            AccountMeta::new(mint.pubkey(), true),
            AccountMeta::new(address(passport_token), false),
            AccountMeta::new_readonly(address(TOKEN_2022_ID), false),
            AccountMeta::new_readonly(address(ASSOCIATED_TOKEN_ID), false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: instruction::CreatePassport {}.data(),
    };
    send(&mut svm, &customer, &[&mint], create_passport.clone()).unwrap();

    let passport_account = svm.get_account(&address(passport)).unwrap();
    let passport_state = Passport::try_deserialize(&mut passport_account.data.as_slice()).unwrap();
    assert_eq!(passport_state.coalition, coalition);
    assert_eq!(passport_state.customer, pubkey(customer.pubkey()));
    assert_eq!(passport_state.passport_mint, pubkey(mint.pubkey()));
    assert_eq!(passport_state.total_visits, 0);
    assert_eq!(passport_state.streak_points, 0);
    assert_eq!(passport_state.version, 1);

    let mint_account = svm.get_account(&mint.pubkey()).unwrap();
    let mint_state = StateWithExtensions::<Mint>::unpack(&mint_account.data).unwrap();
    assert_eq!(mint_state.base.decimals, 0);
    assert_eq!(mint_state.base.supply, 1);
    assert!(mint_state.base.mint_authority.is_none());
    assert!(mint_state
        .get_extension_types()
        .unwrap()
        .contains(&ExtensionType::NonTransferable));

    let token_account = svm.get_account(&passport_token).unwrap();
    let token_state = StateWithExtensions::<TokenAccount>::unpack(&token_account.data).unwrap();
    assert_eq!(token_state.base.owner, pubkey(customer.pubkey()));
    assert_eq!(token_state.base.mint, pubkey(mint.pubkey()));
    assert_eq!(token_state.base.amount, 1);

    let recipient = Keypair::new();
    let recipient_token = get_associated_token_address_with_program_id(
        &pubkey(recipient.pubkey()),
        &pubkey(mint.pubkey()),
        &TOKEN_2022_ID,
    );
    let create_recipient_token = anchor_spl::associated_token::spl_associated_token_account::instruction::create_associated_token_account(
        &pubkey(customer.pubkey()),
        &pubkey(recipient.pubkey()),
        &pubkey(mint.pubkey()),
        &TOKEN_2022_ID,
    );
    send(&mut svm, &customer, &[], create_recipient_token).unwrap();

    let transfer = token_instruction::transfer_checked(
        &TOKEN_2022_ID,
        &passport_token,
        &mint.pubkey(),
        &recipient_token,
        &customer.pubkey(),
        &[],
        1,
        0,
    )
    .unwrap();
    assert!(send(&mut svm, &customer, &[], transfer).is_err());
    let source_after = svm.get_account(&address(passport_token)).unwrap();
    let source_state = StateWithExtensions::<TokenAccount>::unpack(&source_after.data).unwrap();
    let destination_after = svm.get_account(&address(recipient_token)).unwrap();
    let destination_state =
        StateWithExtensions::<TokenAccount>::unpack(&destination_after.data).unwrap();
    assert_eq!(source_state.base.amount, 1);
    assert_eq!(destination_state.base.amount, 0);

    let duplicate_mint = Keypair::new();
    let duplicate_token = get_associated_token_address_with_program_id(
        &pubkey(customer.pubkey()),
        &duplicate_mint.pubkey(),
        &TOKEN_2022_ID,
    );
    let duplicate = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(customer.pubkey(), true),
            AccountMeta::new_readonly(address(coalition), false),
            AccountMeta::new(address(passport), false),
            AccountMeta::new(duplicate_mint.pubkey(), true),
            AccountMeta::new(address(duplicate_token), false),
            AccountMeta::new_readonly(address(TOKEN_2022_ID), false),
            AccountMeta::new_readonly(address(ASSOCIATED_TOKEN_ID), false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: instruction::CreatePassport {}.data(),
    };
    assert!(send(&mut svm, &customer, &[&duplicate_mint], duplicate).is_err());
    assert!(svm.get_account(&duplicate_mint.pubkey()).is_none());

    let second_customer = Keypair::new();
    svm.airdrop(&second_customer.pubkey(), 10_000_000_000)
        .unwrap();
    let second_mint = Keypair::new();
    let (second_passport, _) = Pubkey::find_program_address(
        &[
            b"passport",
            coalition.as_ref(),
            second_customer.pubkey().as_ref(),
        ],
        &ID,
    );
    let wrong_token = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(second_customer.pubkey(), true),
            AccountMeta::new_readonly(address(coalition), false),
            AccountMeta::new(address(second_passport), false),
            AccountMeta::new(second_mint.pubkey(), true),
            AccountMeta::new(customer.pubkey(), false),
            AccountMeta::new_readonly(address(TOKEN_2022_ID), false),
            AccountMeta::new_readonly(address(ASSOCIATED_TOKEN_ID), false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: instruction::CreatePassport {}.data(),
    };
    assert!(send(&mut svm, &second_customer, &[&second_mint], wrong_token,).is_err());
    assert!(svm.get_account(&second_mint.pubkey()).is_none());

    let (balance, _) =
        Pubkey::find_program_address(&[b"balance", passport.as_ref(), merchant.as_ref()], &ID);
    let first_receipt = record_receipt_instruction(
        program_id,
        merchant_authority.pubkey(),
        coalition,
        merchant,
        passport,
        balance,
        1,
        200,
        [7; 32],
    );
    send(&mut svm, &customer, &[&merchant_authority], first_receipt).unwrap();

    let balance_account = svm.get_account(&address(balance)).unwrap();
    let balance_state =
        MerchantBalance::try_deserialize(&mut balance_account.data.as_slice()).unwrap();
    assert_eq!(balance_state.passport, passport);
    assert_eq!(balance_state.merchant, merchant);
    assert_eq!(balance_state.earned_units, 12);
    assert_eq!(balance_state.redeemed_units, 0);
    assert_eq!(balance_state.earned_this_epoch, 12);
    assert_eq!(balance_state.last_receipt_nonce, 1);
    assert_eq!(balance_state.version, 1);
    let passport_account = svm.get_account(&address(passport)).unwrap();
    let passport_state = Passport::try_deserialize(&mut passport_account.data.as_slice()).unwrap();
    assert_eq!(passport_state.total_visits, 1);
    assert_eq!(passport_state.streak_points, 12);

    let before_failed_receipts = (balance_account.data.clone(), passport_account.data.clone());
    svm.expire_blockhash();
    let replay = record_receipt_instruction(
        program_id,
        merchant_authority.pubkey(),
        coalition,
        merchant,
        passport,
        balance,
        1,
        200,
        [7; 32],
    );
    assert!(send(&mut svm, &customer, &[&merchant_authority], replay).is_err());

    let empty_hash = record_receipt_instruction(
        program_id,
        merchant_authority.pubkey(),
        coalition,
        merchant,
        passport,
        balance,
        2,
        100,
        [0; 32],
    );
    assert!(send(&mut svm, &customer, &[&merchant_authority], empty_hash).is_err());

    let exhausted_cap = record_receipt_instruction(
        program_id,
        merchant_authority.pubkey(),
        coalition,
        merchant,
        passport,
        balance,
        2,
        100,
        [8; 32],
    );
    assert!(send(&mut svm, &customer, &[&merchant_authority], exhausted_cap).is_err());

    let wrong_merchant_signer = record_receipt_instruction(
        program_id,
        attacker.pubkey(),
        coalition,
        merchant,
        passport,
        balance,
        2,
        100,
        [9; 32],
    );
    assert!(send(&mut svm, &customer, &[&attacker], wrong_merchant_signer).is_err());
    assert_eq!(
        svm.get_account(&address(balance)).unwrap().data,
        before_failed_receipts.0
    );
    assert_eq!(
        svm.get_account(&address(passport)).unwrap().data,
        before_failed_receipts.1
    );

    let pause_for_receipts = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new_readonly(customer.pubkey(), true),
            AccountMeta::new(address(coalition), false),
        ],
        data: instruction::PauseCoalition {}.data(),
    };
    send(&mut svm, &customer, &[], pause_for_receipts).unwrap();
    let paused_receipt = record_receipt_instruction(
        program_id,
        merchant_authority.pubkey(),
        coalition,
        merchant,
        passport,
        balance,
        2,
        100,
        [10; 32],
    );
    assert!(send(&mut svm, &customer, &[&merchant_authority], paused_receipt).is_err());

    let redeem_five = redeem_instruction(
        program_id,
        customer.pubkey(),
        coalition,
        passport,
        merchant,
        balance,
        5,
    );
    send(&mut svm, &customer, &[], redeem_five).unwrap();
    let balance_after_redeem = svm.get_account(&address(balance)).unwrap();
    let balance_state =
        MerchantBalance::try_deserialize(&mut balance_after_redeem.data.as_slice()).unwrap();
    assert_eq!(balance_state.redeemed_units, 5);
    assert_eq!(balance_state.earned_units - balance_state.redeemed_units, 7);

    let attacker_redeem = redeem_instruction(
        program_id,
        attacker.pubkey(),
        coalition,
        passport,
        merchant,
        balance,
        1,
    );
    assert!(send(&mut svm, &customer, &[&attacker], attacker_redeem).is_err());
    assert_eq!(
        svm.get_account(&address(balance)).unwrap().data,
        balance_after_redeem.data
    );

    let unpause_for_next_day = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new_readonly(customer.pubkey(), true),
            AccountMeta::new(address(coalition), false),
        ],
        data: instruction::UnpauseCoalition {}.data(),
    };
    send(&mut svm, &customer, &[], unpause_for_next_day).unwrap();
    let mut clock = svm.get_sysvar::<Clock>();
    clock.unix_timestamp += 86_400;
    svm.set_sysvar(&clock);
    let next_day_receipt = record_receipt_instruction(
        program_id,
        merchant_authority.pubkey(),
        coalition,
        merchant,
        passport,
        balance,
        2,
        100,
        [11; 32],
    );
    send(
        &mut svm,
        &customer,
        &[&merchant_authority],
        next_day_receipt,
    )
    .unwrap();
    let next_day_balance = svm.get_account(&address(balance)).unwrap();
    let balance_state =
        MerchantBalance::try_deserialize(&mut next_day_balance.data.as_slice()).unwrap();
    assert_eq!(balance_state.earned_units, 22);
    assert_eq!(balance_state.redeemed_units, 5);
    assert_eq!(balance_state.earned_this_epoch, 10);
    assert_eq!(balance_state.last_receipt_nonce, 2);
    let next_day_passport = svm.get_account(&address(passport)).unwrap();
    let passport_state = Passport::try_deserialize(&mut next_day_passport.data.as_slice()).unwrap();
    assert_eq!(passport_state.total_visits, 2);
    assert_eq!(passport_state.streak_points, 22);
}
