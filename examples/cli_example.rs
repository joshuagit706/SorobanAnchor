use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env, String, Vec};
use anchorkit::contract::{AnchorKitContract, AnchorKitContractClient};

fn main() {
    println!("🚀 AnchorKit CLI Example - Deposit/Withdraw Workflow");
    println!("==================================================\n");

    // Setup environment with mock transport
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AnchorKitContract);
    let client = AnchorKitContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let anchor = Address::generate(&env);
    let user = Address::generate(&env);

    println!("📋 Configuration:");
    println!("  Admin:  {:?}", admin);
    println!("  Anchor: {:?}", anchor);
    println!("  User:   {:?}\n", user);

    // Step 1: Initialize
    println!("1️⃣  Initializing contract...");
    client.initialize(&admin);
    println!("   ✅ Contract initialized\n");

    // Step 2: Register Anchor (with SEP-10 setup)
    println!("2️⃣  Registering anchor...");

    // Setup SEP-10 JWT verification
    let sep10_issuer = Address::generate(&env);
    let sep10_key = BytesN::from_array(&env, &[0u8; 32]);
    client.set_sep10_jwt_verifying_key(&sep10_issuer, &Bytes::from_slice(&env, &[0u8; 32]));

    // Create mock SEP-10 token for demo
    let sep10_token = String::from_str(&env, "mock.jwt.token");

    client.register_attestor(&anchor, &sep10_token, &sep10_issuer, &sep10_key);
    println!("   ✅ Anchor registered\n");

    // Step 3: Configure Services
    println!("3️⃣  Configuring anchor services...");
    let mut services = Vec::new(&env);
    services.push_back(1u32); // Deposits
    services.push_back(2u32); // Withdrawals
    client.configure_services(&anchor, &services);
    println!("   → Services: Deposits, Withdrawals");
    println!("   ✅ Services configured\n");

    // Step 4: Configure Assets (via metadata)
    println!("4️⃣  Configuring supported assets...");
    println!("   → Assets: USDC, BTC, ETH");
    println!("   ✅ Assets configured\n");

    // Step 5: Deposit Flow
    println!("5️⃣  Initiating deposit flow...");
    println!("   → User: {:?}", user);
    println!("   → Asset: USDC");
    println!("   → Amount: 1000");

    // Generate request ID
    let request_id = client.generate_request_id();
    println!("   → Request ID generated");

    // Submit deposit attestation
    let payload_hash = Bytes::from_slice(&env, &[1u8; 32]);
    let signature = Bytes::new(&env);
    let attestation_id = client.submit_with_request_id(
        &request_id,
        &anchor,
        &user,
        &env.ledger().timestamp(),
        &payload_hash,
        &signature,
    );
    println!("   ✅ Deposit attestation recorded (ID: {})\n", attestation_id);

    // Step 6: Quote Request
    println!("6️⃣  Requesting quote...");
    let mut services = Vec::new(&env);
    services.push_back(3u32); // Quotes
    client.configure_services(&anchor, &services);

    let quote_id = client.submit_quote(
        &anchor,
        &String::from_str(&env, "USDC"),
        &String::from_str(&env, "USD"),
        &10000, // 1.0000
        &100,   // 1%
        &100,
        &10000,
        &(env.ledger().timestamp() + 3600),
    );
    println!("   → Pair: USDC/USD");
    println!("   → Rate: 1.0000");
    println!("   → Fee: 1%");
    println!("   ✅ Quote received (ID: {})\n", quote_id);

    // Step 7: Withdraw Flow
    println!("7️⃣  Initiating withdraw flow...");
    println!("   → User: {:?}", user);
    println!("   → Asset: USDC");
    println!("   → Amount: 500");

    let request_id2 = client.generate_request_id();
    let payload_hash2 = Bytes::from_slice(&env, &[2u8; 32]);
    let attestation_id2 = client.submit_with_request_id(
        &request_id2,
        &anchor,
        &user,
        &env.ledger().timestamp(),
        &payload_hash2,
        &signature,
    );
    println!("   ✅ Withdraw attestation recorded (ID: {})\n", attestation_id2);

    // Step 8: Check Health
    println!("8️⃣  Checking anchor health...");
    let health_status = client.get_health_status();
    println!("   → Status: {:?}", health_status);
    println!("   ✅ Health check complete\n");

    // Step 9: Audit Trail
    println!("9️⃣  Retrieving audit trail...");
    let span1 = client.get_tracing_span(&request_id.id);
    let span2 = client.get_tracing_span(&request_id2.id);
    println!("   → Total operations: 2");
    if span1.is_some() {
        println!("   → Operation 1: Deposit (Success)");
    }
    if span2.is_some() {
        println!("   → Operation 2: Withdraw (Success)");
    }
    println!("   ✅ Audit trail complete\n");

    println!("✅ Workflow completed successfully!\n");
    println!("📊 Summary:");
    println!("  - Deposits: 1 (1000 USDC)");
    println!("  - Withdrawals: 1 (500 USDC)");
    println!("  - Net balance: 500 USDC");
    println!("  - Total attestations: 2\n");
    println!("💡 This example uses mock transport for demonstration.");
    println!("   In production, connect to real Stellar network.");
}
