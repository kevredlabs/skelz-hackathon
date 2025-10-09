#![allow(deprecated)]
// Temporary: Anchor macro emits a deprecated realloc; safe to ignore here
use anchor_lang::prelude::*;
use sha2::{Sha256, Digest};

declare_id!("4uw8DwTRdUMwGmbNrK5GZ5kgdVtco4aUaTGDnEUBrYKt");

#[program]
pub mod skelz {
    use super::*;

    pub fn write_signature(ctx: Context<WriteSignature>, digest: String) -> Result<()> {
        let signature = &mut ctx.accounts.signature;
        signature.digest = digest;
        signature.signer = ctx.accounts.signer.key();
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(digest: String)]
pub struct WriteSignature<'info> {
    // Keep one signer to be explicit and future-proof if needed
    #[account(mut,signer)]
    pub signer: Signer<'info>,
    #[account(
    init,
    payer = signer,
    space = 8 + 100 + 32,
    seeds = [b"signature", &Sha256::digest(digest.as_bytes())[..]],
    bump)]
    pub signature: Account<'info, Signature>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct Signature {
    pub digest: String,
    pub signer: Pubkey,
}
