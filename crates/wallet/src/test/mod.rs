use crossbeam_channel::unbounded;
use moneymq_types::RecurringScheme;
use moneymq_types::x402::Currency;
use moneymq_types::x402::Network;
use solana_keypair::Signer;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_transaction::versioned::VersionedTransaction;
use surfpool_core::solana_rpc_client::rpc_client;
use surfpool_types::BlockProductionMode;

use crate::MoneyMqWallet;

mod remote_signer;

use remote_signer::RemoteSigner;

fn start_surfpool(airdrop_addresses: &Vec<solana_pubkey::Pubkey>) -> String {
    use surfpool_core::surfnet::svm::SurfnetSvm;
    use surfpool_core::{runloops::start_local_surfnet_runloop, surfnet::locker::SurfnetSvmLocker};
    use surfpool_types::SimnetEvent;
    use surfpool_types::{RpcConfig, SimnetConfig, SurfpoolConfig};

    let bind_host = "127.0.0.1";
    let bind_port = 8899;
    let config = SurfpoolConfig {
        simnets: vec![SimnetConfig {
            slot_time: 50,
            airdrop_addresses: airdrop_addresses.clone(),
            airdrop_token_amount: 1_000_000_000,
            // block_production_mode: BlockProductionMode::Transaction,
            ..SimnetConfig::default()
        }],
        rpc: RpcConfig {
            bind_host: bind_host.to_string(),
            bind_port,
            ..Default::default()
        },
        ..SurfpoolConfig::default()
    };

    let (surfnet_svm, simnet_events_rx, geyser_events_rx) = SurfnetSvm::new();
    let (simnet_commands_tx, simnet_commands_rx) = unbounded();
    let (subgraph_commands_tx, _subgraph_commands_rx) = unbounded();
    let svm_locker = SurfnetSvmLocker::new(surfnet_svm);

    let _handle = hiro_system_kit::thread_named("test").spawn(move || {
        let future = start_local_surfnet_runloop(
            svm_locker,
            config,
            subgraph_commands_tx,
            simnet_commands_tx,
            simnet_commands_rx,
            geyser_events_rx,
        );
        if let Err(e) = hiro_system_kit::nestable_block_on(future) {
            panic!("{e:?}");
        }
    });

    let mut ready = false;
    let mut connected = false;
    loop {
        match simnet_events_rx.recv() {
            Ok(SimnetEvent::Ready) => {
                ready = true;
            }
            Ok(SimnetEvent::Connected(_)) => {
                connected = true;
            }
            _ => (),
        }
        if ready && connected {
            break;
        }
    }
    format!("http://{}:{}", bind_host, bind_port)
}

#[test]
fn test_create_swig_wallet_sub_account() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        println!("================ Starting test_create_swig_wallet ================");
        let remote_authority = RemoteSigner::new(1111);
        remote_authority.start_service().await.unwrap();
        let authority_pubkey = remote_authority.get_pubkey().await;
        println!("Authority pubkey: {}", authority_pubkey);

        let remote_fee_payer = RemoteSigner::new(2222);
        remote_fee_payer.start_service().await.unwrap();
        let fee_payer_pubkey = remote_fee_payer.get_pubkey().await;
        println!("Fee payer pubkey: {}", fee_payer_pubkey);

        let swig_id = authority_pubkey.to_bytes();

        let rpc_url = start_surfpool(&vec![authority_pubkey, fee_payer_pubkey]);
        let rpc_client = rpc_client::RpcClient::new(rpc_url.clone());

        let mut wallet = MoneyMqWallet::new_ed25519_wallet(
            swig_id,
            &remote_authority,
            &rpc_url,
            fee_payer_pubkey,
        )
        .unwrap();

        // Initialize authority permissions
        {
            println!("\n\n========= Initializing authority permissions =========");
            let mut initialize_authority_tx = wallet
                .get_set_permissions_transaction(
                    vec![Currency::from_symbol_and_network("USDC", &Network::Solana).unwrap()],
                    RecurringScheme::Monthly(1),
                    1_000_000,
                    None,
                )
                .unwrap();
            println!(
                "\nInitialize authority transaction before signing: {:?}",
                initialize_authority_tx
            );
            partial_sign_transaction(&[&remote_fee_payer], &mut initialize_authority_tx);
            println!(
                "\nInitialize authority transaction after wallet signing: {:?}",
                initialize_authority_tx
            );
            let sig = rpc_client
                .send_and_confirm_transaction(&initialize_authority_tx)
                .unwrap();
            println!("\nInitialize authority transaction signature: {}", sig);
        }

        wallet.display_swig().unwrap();

        // Create sub-account
        let sub_account = {
            println!("\n\n========= Creating sub-account =========");
            let mut tx = wallet.get_create_sub_account_transaction().unwrap();
            println!("\nCreate sub-account transaction before signing: {:?}", tx);
            partial_sign_transaction(&[&remote_fee_payer], &mut tx);
            println!(
                "\nCreate sub-account transaction after wallet signing: {:?}",
                tx
            );
            let sig = rpc_client.send_and_confirm_transaction(&tx).unwrap();
            println!("\nCreate sub-account transaction signature: {}", sig);
            let sub_account = wallet.get_sub_account().unwrap().unwrap();
            println!("Sub-account pubkey: {}", sub_account);
            rpc_client
                .request_airdrop(&sub_account, 2_000_000_000)
                .unwrap();

            wallet.display_swig().unwrap();
            sub_account
        };

        let receiver_pubkey = Pubkey::new_unique();
        println!("Receiver pubkey: {}", receiver_pubkey);

        // Transfer lamports from sub-account to receiver
        {
            println!("\n\n========= Initiating Transfer =========");

            println!(
                "Swig wallet balance before: {}",
                wallet.get_balance().unwrap()
            );
            println!(
                "Sub-account balance before: {}",
                rpc_client.get_balance(&sub_account).unwrap()
            );
            println!(
                "Receiver balance before: {}",
                rpc_client.get_balance(&receiver_pubkey).unwrap()
            );

            // let transfer = transfer(&sub_account, &receiver_pubkey, 1);
            // wallet.sign_with_sub_account(vec![transfer], None).unwrap();

            // Sign with external fee payer
            let mut transfer_tx = wallet
                .get_transfer_sub_account_tx(&sub_account, &receiver_pubkey, 1_000_000_000)
                .unwrap();

            println!("\nPartially signed transaction: {:?}", transfer_tx);
            partial_sign_transaction(&[&remote_fee_payer], &mut transfer_tx);

            println!("\nTransaction: {:?}", transfer_tx);
            let sig = rpc_client
                .send_and_confirm_transaction(&transfer_tx)
                .unwrap();
            println!("\nTransaction signature: {}", sig);

            println!(
                "\nSwig wallet balance after: {}",
                wallet.get_balance().unwrap()
            );
            println!(
                "Sub-account balance after: {}",
                rpc_client.get_balance(&sub_account).unwrap()
            );
            println!(
                "Receiver balance after: {}",
                rpc_client.get_balance(&receiver_pubkey).unwrap()
            );
        }
    }); // end rt.block_on
}

#[test]
fn test_create_swig_wallet() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        println!("================ Starting test_create_swig_wallet ================");
        let remote_authority = RemoteSigner::new(1111);
        remote_authority.start_service().await.unwrap();
        let authority_pubkey = remote_authority.get_pubkey().await;
        println!("Authority pubkey: {}", authority_pubkey);

        let remote_fee_payer = RemoteSigner::new(2222);
        remote_fee_payer.start_service().await.unwrap();
        let fee_payer_pubkey = remote_fee_payer.get_pubkey().await;
        println!("Fee payer pubkey: {}", fee_payer_pubkey);

        let swig_id = authority_pubkey.to_bytes();

        let rpc_url = start_surfpool(&vec![authority_pubkey, fee_payer_pubkey]);
        let rpc_client = rpc_client::RpcClient::new(rpc_url.clone());

        let mut wallet = MoneyMqWallet::new_ed25519_wallet(
            swig_id,
            &remote_authority,
            &rpc_url,
            fee_payer_pubkey,
        )
        .unwrap();

        // Initialize authority permissions
        {
            println!("\n\n========= Initializing authority permissions =========");
            let mut initialize_authority_tx = wallet
                .get_set_permissions_transaction(
                    vec![Currency::from_symbol_and_network("USDC", &Network::Solana).unwrap()],
                    RecurringScheme::Monthly(1),
                    1_000_000,
                    None,
                )
                .unwrap();
            println!(
                "\nInitialize authority transaction before signing: {:?}",
                initialize_authority_tx
            );
            partial_sign_transaction(&[&remote_fee_payer], &mut initialize_authority_tx);
            println!(
                "\nInitialize authority transaction after wallet signing: {:?}",
                initialize_authority_tx
            );
            let sig = rpc_client
                .send_and_confirm_transaction(&initialize_authority_tx)
                .unwrap();
            println!("\nInitialize authority transaction signature: {}", sig);
        }

        wallet.display_swig().unwrap();

        // Create sub-account
        // {
        //     println!("\n\n========= Creating sub-account =========");
        //     let mut tx = wallet.get_create_sub_account_transaction().unwrap();
        //     println!("\nCreate sub-account transaction before signing: {:?}", tx);
        //     partial_sign_transaction(&[&remote_fee_payer], &mut tx);
        //     println!(
        //         "\nCreate sub-account transaction after wallet signing: {:?}",
        //         tx
        //     );
        //     let sig = rpc_client.send_and_confirm_transaction(&tx).unwrap();
        //     println!("\nCreate sub-account transaction signature: {}", sig);
        // let sub_account = wallet.get_sub_account().unwrap().unwrap();
        // println!("Sub-account pubkey: {}", sub_account);
        // rpc_client
        //     .request_airdrop(&sub_account, 2_000_000_000)
        //     .unwrap();

        // wallet.display_swig().unwrap();
        // }

        let receiver_pubkey = Pubkey::new_unique();
        println!("Receiver pubkey: {}", receiver_pubkey);

        // Transfer lamports from sub-account to receiver
        {
            println!("\n\n========= Initiating Transfer =========");

            let swig_wallet_address = wallet.get_swig_wallet_address().unwrap();
            rpc_client
                .request_airdrop(&swig_wallet_address, 2_000_000_000)
                .unwrap();
            println!("Swig wallet address: {}", swig_wallet_address);
            println!(
                "Swig wallet balance before: {}",
                wallet.get_balance().unwrap()
            );
            // println!(
            //     "Sub-account balance before: {}",
            //     rpc_client.get_balance(&sub_account).unwrap()
            // );
            println!(
                "Swig wallet address balance before: {}",
                rpc_client.get_balance(&swig_wallet_address).unwrap()
            );
            println!(
                "Receiver balance before: {}",
                rpc_client.get_balance(&receiver_pubkey).unwrap()
            );

            // let transfer = transfer(&sub_account, &receiver_pubkey, 1);
            // wallet.sign_with_sub_account(vec![transfer], None).unwrap();

            // Sign with external fee payer
            let mut transfer_tx = wallet
                .get_transfer_tx(
                    &wallet.get_swig_wallet_address().unwrap(),
                    &receiver_pubkey,
                    1_000_000_000,
                )
                .unwrap();

            println!("\nPartially signed transaction: {:?}", transfer_tx);
            partial_sign_transaction(&[&remote_fee_payer], &mut transfer_tx);

            println!("\nTransaction: {:?}", transfer_tx);
            let sig = rpc_client
                .send_and_confirm_transaction(&transfer_tx)
                .unwrap();
            println!("\nTransaction signature: {}", sig);

            println!(
                "\nSwig wallet balance after: {}",
                wallet.get_balance().unwrap()
            );
            // println!(
            //     "Sub-account balance after: {}",
            //     rpc_client.get_balance(&sub_account).unwrap()
            // );
            println!(
                "Swig wallet address balance after: {}",
                rpc_client.get_balance(&swig_wallet_address).unwrap()
            );
            println!(
                "Receiver balance after: {}",
                rpc_client.get_balance(&receiver_pubkey).unwrap()
            );
        }
    }); // end rt.block_on
}

fn partial_sign_transaction(signers: &[&dyn Signer], tx: &mut VersionedTransaction) {
    let message_bytes = tx.message.serialize();
    for signer in signers {
        let signer_pubkey = signer.pubkey();
        let position = tx
            .message
            .static_account_keys()
            .iter()
            .position(|&k| k == signer_pubkey)
            .unwrap();
        let signature = signer.sign_message(&message_bytes);
        tx.signatures[position] = signature;
    }
}
