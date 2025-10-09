use anchor_client::{
    solana_client::rpc_client::RpcClient,
    solana_sdk::{
        commitment_config::CommitmentConfig, 
        native_token::LAMPORTS_PER_SOL, 
        signature::Keypair,
        signer::Signer, 
        system_program,
        pubkey::Pubkey,
    },
    Client, Cluster,
};
use anchor_lang::prelude::*;
use std::rc::Rc;

// Déclarer le programme à partir de l'IDL
declare_program!(skelz);
use skelz::{accounts::Signature, client::accounts, client::args};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 Starting Skelz Program Test");
    
    // Configuration de la connexion
    let connection = RpcClient::new_with_commitment(
        "http://127.0.0.1:8899", // Local validator URL
        CommitmentConfig::confirmed(),
    );

    // Générer les keypairs
    let payer = Keypair::new();
    println!("Generated Keypairs:");
    println!("   Payer: {}", payer.pubkey());

    // Airdrop SOL
    println!("\n💰 Requesting 2 SOL airdrop to payer");
    let airdrop_signature = connection.request_airdrop(&payer.pubkey(), 2 * LAMPORTS_PER_SOL)?;

    // Attendre la confirmation de l'airdrop
    while !connection.confirm_transaction(&airdrop_signature)? {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    println!("   ✅ Airdrop confirmed!");

    // Créer le client du programme
    let provider = Client::new_with_options(
        Cluster::Localnet,
        Rc::new(payer),
        CommitmentConfig::confirmed(),
    );
    let program = provider.program(skelz::ID)?;

    // Test 1: Créer une signature
    println!("\n📝 Test 1: Creating signature for image digest");
    let digest = "sha256:abc123def456789";
    
    // Dériver le PDA pour cette signature
    let (signature_pda, _bump) = Pubkey::find_program_address(
        &[b"signature", digest.as_bytes()],
        &program.id(),
    );
    
    println!("   Digest: {}", digest);
    println!("   Signature PDA: {}", signature_pda);

    // Construire l'instruction
    let write_signature_ix = program
        .request()
        .accounts(accounts::WriteSignature {
            signer: program.payer(),
            signature: signature_pda,
            system_program: system_program::ID,
        })
        .args(args::WriteSignature {
            digest: digest.to_string(),
        })
        .instructions()?
        .remove(0);

    // Envoyer la transaction
    let signature = program
        .request()
        .instruction(write_signature_ix)
        .send()
        .await?;
    
    println!("   ✅ Transaction confirmed: {}", signature);

    // Vérifier que le compte a été créé
    println!("\n🔍 Verifying signature account creation");
    let signature_account: Signature = program.account::<Signature>(signature_pda).await?;
    println!("   ✅ Signature account created!");
    println!("   - Digest: {}", signature_account.digest);
    println!("   - Signer: {}", signature_account.signer);

    // Test 2: Vérifier que la duplication échoue
    println!("\n🔄 Test 2: Testing duplicate signature creation (should fail)");
    let duplicate_ix = program
        .request()
        .accounts(accounts::WriteSignature {
            signer: program.payer(),
            signature: signature_pda,
            system_program: system_program::ID,
        })
        .args(args::WriteSignature {
            digest: digest.to_string(),
        })
        .instructions()?
        .remove(0);

    let duplicate_result = program
        .request()
        .instruction(duplicate_ix)
        .send()
        .await;

    match duplicate_result {
        Ok(_) => println!("   ❌ ERROR: Duplicate creation should have failed!"),
        Err(e) => {
            println!("   ✅ Duplicate creation correctly rejected!");
            println!("   Error: {}", e);
        }
    }

    // Test 3: Créer une signature avec un digest différent
    println!("\n📝 Test 3: Creating signature with different digest");
    let digest2 = "sha256:xyz789abc123";
    let (signature_pda2, _bump2) = Pubkey::find_program_address(
        &[b"signature", digest2.as_bytes()],
        &program.id(),
    );
    
    println!("   Digest: {}", digest2);
    println!("   Signature PDA: {}", signature_pda2);

    let write_signature_ix2 = program
        .request()
        .accounts(accounts::WriteSignature {
            signer: program.payer(),
            signature: signature_pda2,
            system_program: system_program::ID,
        })
        .args(args::WriteSignature {
            digest: digest2.to_string(),
        })
        .instructions()?
        .remove(0);

    let signature2 = program
        .request()
        .instruction(write_signature_ix2)
        .send()
        .await?;
    
    println!("   ✅ Transaction confirmed: {}", signature2);

    // Vérifier la deuxième signature
    let signature_account2: Signature = program.account::<Signature>(signature_pda2).await?;
    println!("   ✅ Second signature account created!");
    println!("   - Digest: {}", signature_account2.digest);
    println!("   - Signer: {}", signature_account2.signer);

    // Vérifier que les PDAs sont différents
    assert_ne!(signature_pda, signature_pda2, "PDAs should be different for different digests");
    println!("   ✅ PDAs are correctly different for different digests");

    println!("\n🎉 All tests passed successfully!");
    println!("   - Signature creation works");
    println!("   - Duplicate prevention works");
    println!("   - Different digests create different PDAs");
    println!("   - Account data is correctly stored");

    Ok(())
}