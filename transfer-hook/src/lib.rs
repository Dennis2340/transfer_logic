use anchor_lang::{
    prelude::*,
    system_program::{create_account, CreateAccount},
};
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use spl_tlv_account_resolution::{
    state::ExtraAccountMetaList,
};
use spl_transfer_hook_interface::instruction::{ExecuteInstruction, TransferHookInstruction};

declare_id!("8BZPRLCsb7NRKwr83CuzErr7HdcB8imhk6BJAetAJgbF");

#[program]
pub mod transfer_hook {
    use super::*;

    // Constants for royalty percentage (e.g., 5%)
    const ROYALTY_PERCENTAGE: u64 = 5;

    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {

        // The addExtraAccountsToInstruction JS helper function resolving incorrectly
        let account_metas = vec![];

        // Calculate account size
        let account_size = ExtraAccountMetaList::size_of(account_metas.len())? as u64;
        // Calculate minimum required lamports
        let lamports = Rent::get()?.minimum_balance(account_size as usize);

        let mint = ctx.accounts.mint.key();
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"extra-account-metas",
            &mint.as_ref(),
            &[ctx.bumps.extra_account_meta_list],
        ]];

        // Create ExtraAccountMetaList account
        create_account(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                CreateAccount {
                    from: ctx.accounts.payer.to_account_info(),
                    to: ctx.accounts.extra_account_meta_list.to_account_info(),
                },
            )
            .with_signer(signer_seeds),
            lamports,
            account_size,
            ctx.program_id,
        )?;

        // Initialize ExtraAccountMetaList account with extra accounts
        ExtraAccountMetaList::init::<ExecuteInstruction>(
            &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
            &account_metas,
        )?;

        Ok(())
    }

    pub fn transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
    msg!("Performing on-chain royalties logic in transfer hook!");

    // Calculate the royalty amount and remaining transfer amount
    let royalty_amount = amount * ROYALTY_PERCENTAGE / 100;
    let transfer_amount = amount - royalty_amount;

    // Transfer royalty to the royalty recipient
    let cpi_accounts = anchor_spl::token::Transfer {
        from: ctx.accounts.source_token.to_account_info(),
        to: ctx.accounts.royalty_token_account.to_account_info(),
        authority: ctx.accounts.owner.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info(); // Reference the token program from the context
    anchor_spl::token::transfer(
        CpiContext::new(cpi_program.clone(), cpi_accounts), // Clone the cpi_program here
        royalty_amount,
    )?;

    // Transfer the remaining amount to the destination token account
    let cpi_accounts_transfer = anchor_spl::token::Transfer {
        from: ctx.accounts.source_token.to_account_info(),
        to: ctx.accounts.destination_token.to_account_info(),
        authority: ctx.accounts.owner.to_account_info(),
    };
    anchor_spl::token::transfer(
        CpiContext::new(cpi_program, cpi_accounts_transfer), // No need to clone here again, it's already used
        transfer_amount,
    )?;

    msg!("Royalty transfer complete: {} lamports to royalty recipient", royalty_amount);
    msg!("Remaining transfer complete: {} lamports to destination", transfer_amount);

    Ok(())
}

    // Fallback instruction handler as workaround to anchor instruction discriminator check
    pub fn fallback<'info>(
        program_id: &Pubkey,
        accounts: &'info [AccountInfo<'info>],
        data: &[u8],
    ) -> Result<()> {
        let instruction = TransferHookInstruction::unpack(data)?;

        // Match instruction discriminator to transfer hook interface execute instruction  
        // token2022 program CPIs this instruction on token transfer
        match instruction {
            TransferHookInstruction::Execute { amount } => {
                let amount_bytes = amount.to_le_bytes();

                // Invoke custom transfer hook instruction on our program
                __private::__global::transfer_hook(program_id, accounts, &amount_bytes)
            }
            _ => return Err(ProgramError::InvalidInstructionData.into()),
        }
    }
}

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut)]
    payer: Signer<'info>,

    /// CHECK: ExtraAccountMetaList Account, must use these seeds
    #[account(
        mut,
        seeds = [b"extra-account-metas", mint.key().as_ref()], 
        bump
    )]
    pub extra_account_meta_list: AccountInfo<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>, // Add token_program field here
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// Order of accounts matters for this struct.
// The first 4 accounts are the accounts required for token transfer (source, mint, destination, owner)
// Remaining accounts are the extra accounts required from the ExtraAccountMetaList account
// These accounts are provided via CPI to this program from the token2022 program
#[derive(Accounts)]
pub struct TransferHook<'info> {
    #[account(
        token::mint = mint, 
        token::authority = owner,
    )]
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        token::mint = mint,
    )]
    pub destination_token: InterfaceAccount<'info, TokenAccount>,
    #[account(
        token::mint = mint,
    )]
    pub royalty_token_account: InterfaceAccount<'info, TokenAccount>, // Royalty recipient token account
    /// CHECK: source token account owner, can be SystemAccount or PDA owned by another program
    pub owner: UncheckedAccount<'info>,
    /// CHECK: ExtraAccountMetaList Account,
    #[account(
        seeds = [b"extra-account-metas", mint.key().as_ref()], 
        bump
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,
    pub token_program: Interface<'info, TokenInterface>, // Add token_program here
}
