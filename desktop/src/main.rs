//! SnartNet desktop host – wires `CoreService` to the native SQLite backend.
//!
//! This binary demonstrates the full end-to-end flow:
//!   1. Open (or create) the SQLite database.
//!   2. Initialise `CoreService<SqliteStorage>` and reload any persisted state.
//!   3. Create a new profile (or display the existing one).
//!   4. Persist the result and print a human-readable summary.

use snartnet_core::{CoreService, SqliteStorage};

fn main() {
    // ------------------------------------------------------------------ setup
    // Use a dedicated database file next to the binary, or a temp path when
    // running under `cargo run`.
    let db_path = std::env::var("SNARTNET_DB").unwrap_or_else(|_| "snartnet.db".into());

    // Initialise the SQLite database at the chosen path.
    SqliteStorage::open(&db_path).expect("failed to open SQLite database");

    // --------------------------------------------------------- CoreService init
    let mut svc = CoreService::<SqliteStorage>::new();
    svc.init().expect("CoreService::init failed");

    // ------------------------------------------------------- profile lifecycle
    if svc.has_profile() {
        let env = svc.get_profile().unwrap();
        println!("=== Existing profile loaded ===");
        println!("  username     : {}", env.profile.username);
        println!(
            "  display_name : {}",
            env.profile.display_name.as_deref().unwrap_or("<none>")
        );
        println!("  fingerprint  : {}", env.profile.fingerprint);
        println!("  magnet URI   : {}", env.magnet_uri);
    } else {
        println!("=== Creating new profile ===");

        let magnet = svc
            .create_profile(
                "alice",
                Some("Alice (desktop)".into()),
                Some("First native SnartNet user!".into()),
            )
            .expect("create_profile failed");

        let env = svc.get_profile().expect("profile missing after creation");

        println!("  username     : {}", env.profile.username);
        println!(
            "  display_name : {}",
            env.profile.display_name.as_deref().unwrap_or("<none>")
        );
        println!("  fingerprint  : {}", env.profile.fingerprint);
        println!("  magnet URI   : {magnet}");
        println!();
        println!("Profile persisted to {db_path}");

        // Verify signature
        let signed = svc.get_signed_profile().expect("no signed profile");
        let ok = signed
            .verify()
            .expect("signature verification failed");
        println!("  signature OK : {ok}");
    }
}
