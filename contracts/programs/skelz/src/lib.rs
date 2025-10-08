#![allow(deprecated)]
// Temporary: Anchor macro emits a deprecated realloc; safe to ignore here
use anchor_lang::prelude::*;

declare_id!("11111111111111111111111111111111");

#[program]
pub mod skelz {
    use super::*;

    pub fn hello(_ctx: Context<Hello>) -> Result<()> {
        msg!("Hello, Skelz!");
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Hello<'info> {
    // Keep one signer to be explicit and future-proof if needed
    #[account(signer)]
    pub signer: Signer<'info>,
}
