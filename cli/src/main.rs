use clap::{Parser, Subcommand};
use snartnet_core::{
    KeyPair,
    Post,
    SignedPost,
    Profile,
    SignedProfile,
    FileStorage,
};

// ---------------------------------------------------------------------------
// CLI top-level
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "snartnet",
    version = env!("CARGO_PKG_VERSION"),
    about = "SnartNet — decentralized social & messaging protocol",
    long_about = None,
)]
struct Cli {
    /// Override the storage directory (default: ~/.snartnet/data)
    #[arg(long, global = true, env = "SNARTNET_DATA_DIR")]
    data_dir: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new SnartNet identity
    Init {
        /// Username (3–32 characters, alphanumeric + underscore)
        username: String,
        /// Display name shown to other users
        #[arg(short = 'n', long)]
        name: Option<String>,
        /// Short profile biography
        #[arg(short, long)]
        bio: Option<String>,
    },

    /// Profile management
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },

    /// Post management
    Post {
        #[command(subcommand)]
        action: PostAction,
    },

    /// Key management
    Keys {
        #[command(subcommand)]
        action: KeysAction,
    },
}

#[derive(Subcommand)]
enum ProfileAction {
    /// Display the current profile
    Show,
    /// Edit profile fields
    Edit {
        /// New display name
        #[arg(short = 'n', long)]
        name: Option<String>,
        /// New biography
        #[arg(short, long)]
        bio: Option<String>,
    },
}

#[derive(Subcommand)]
enum PostAction {
    /// Create a new post
    Create {
        /// Post content
        content: String,
        /// Comma-separated hashtags (without #)
        #[arg(short, long)]
        tags: Option<String>,
        /// Post ID to reply to
        #[arg(short, long)]
        reply_to: Option<String>,
    },
}

#[derive(Subcommand)]
enum KeysAction {
    /// Display public key and fingerprint
    Show,
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();

    let storage = open_storage(cli.data_dir.as_deref());

    let result = match cli.command {
        Commands::Init { username, name, bio } => cmd_init(&storage, &username, name, bio),
        Commands::Profile { action } => match action {
            ProfileAction::Show => cmd_profile_show(&storage),
            ProfileAction::Edit { name, bio } => cmd_profile_edit(&storage, name, bio),
        },
        Commands::Post { action } => match action {
            PostAction::Create { content, tags, reply_to } => {
                cmd_post_create(&storage, &content, tags, reply_to)
            }
        },
        Commands::Keys { action } => match action {
            KeysAction::Show => cmd_keys_show(&storage),
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

fn open_storage(data_dir: Option<&str>) -> FileStorage {
    let storage = match data_dir {
        Some(dir) => FileStorage::new(dir),
        None => FileStorage::open_default(),
    };
    storage.unwrap_or_else(|e| {
        eprintln!("Fatal: could not open storage: {e}");
        std::process::exit(1);
    })
}

fn load_keypair(storage: &FileStorage) -> Result<KeyPair, String> {
    storage
        .get_json::<KeyPair>("keypair")
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No identity found. Run `snartnet init <username>` first.".to_string())
}

fn load_profile(storage: &FileStorage) -> Result<SignedProfile, String> {
    storage
        .get_json::<SignedProfile>("profile")
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No profile found. Run `snartnet init <username>` first.".to_string())
}

fn save_keypair(storage: &FileStorage, kp: &KeyPair) -> Result<(), String> {
    storage.set_json("keypair", kp).map_err(|e| e.to_string())
}

fn save_profile(storage: &FileStorage, sp: &SignedProfile) -> Result<(), String> {
    storage.set_json("profile", sp).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_init(
    storage: &FileStorage,
    username: &str,
    display_name: Option<String>,
    bio: Option<String>,
) -> Result<(), String> {
    if storage.get_item("keypair").map_err(|e| e.to_string())?.is_some() {
        return Err("An identity already exists. To start fresh, remove ~/.snartnet/data/".to_string());
    }

    validate_username(username)?;

    let keypair = KeyPair::generate()?;
    let key_info = keypair.get_public_info();

    let mut profile = Profile::new(username.to_string(), key_info);
    if display_name.is_some() || bio.is_some() {
        profile.update(display_name, bio);
    }

    let mut signed = SignedProfile::create(profile, &keypair)?;
    let magnet = signed.profile.generate_magnet_uri();
    signed.profile.magnet_uri = Some(magnet.clone());

    save_keypair(storage, &keypair)?;
    save_profile(storage, &signed)?;

    println!("✓ Identity created for @{username}");
    println!("  Fingerprint : {}", keypair.fingerprint);
    println!("  Magnet URI  : {magnet}");
    Ok(())
}

fn cmd_profile_show(storage: &FileStorage) -> Result<(), String> {
    let sp = load_profile(storage)?;
    let p = &sp.profile;
    println!("Username    : @{}", p.username);
    if let Some(ref name) = p.display_name {
        println!("Name        : {name}");
    }
    if let Some(ref bio) = p.bio {
        println!("Bio         : {bio}");
    }
    println!("Fingerprint : {}", p.fingerprint);
    println!("Version     : {}", p.version);
    println!("Created     : {}", p.created_at.format("%Y-%m-%d %H:%M UTC"));
    println!("Updated     : {}", p.updated_at.format("%Y-%m-%d %H:%M UTC"));
    if let Some(ref uri) = p.magnet_uri {
        println!("Magnet URI  : {uri}");
    }
    Ok(())
}

fn cmd_profile_edit(
    storage: &FileStorage,
    display_name: Option<String>,
    bio: Option<String>,
) -> Result<(), String> {
    if display_name.is_none() && bio.is_none() {
        return Err("Provide at least --name or --bio to update".to_string());
    }
    let kp = load_keypair(storage)?;
    let mut sp = load_profile(storage)?;
    sp.profile.update(display_name, bio);

    let new_signed = SignedProfile::create(sp.profile.clone(), &kp)?;
    let mut new_signed = new_signed;
    new_signed.profile.magnet_uri = Some(new_signed.profile.generate_magnet_uri());

    save_profile(storage, &new_signed)?;
    println!("✓ Profile updated (version {})", new_signed.profile.version);
    Ok(())
}

fn cmd_post_create(
    storage: &FileStorage,
    content: &str,
    tags_raw: Option<String>,
    reply_to: Option<String>,
) -> Result<(), String> {
    let kp = load_keypair(storage)?;
    let sp = load_profile(storage)?;

    let tags: Option<Vec<String>> = tags_raw.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });

    let post = Post::new(
        sp.profile.fingerprint.clone(),
        content.to_string(),
        tags,
        reply_to,
    );

    let signed = SignedPost::create(post, &kp)?;

    // Persist the signed post under a key derived from its ID.
    let post_key = format!("post_{}", signed.post.id);
    storage
        .set_json(&post_key, &signed)
        .map_err(|e| e.to_string())?;

    println!("✓ Post created");
    println!("  ID          : {}", signed.post.id);
    println!("  Author      : {}", signed.post.author_fingerprint);
    println!("  Content     : {}", signed.post.content);
    if !signed.post.tags.is_empty() {
        println!("  Tags        : #{}", signed.post.tags.join(" #"));
    }
    Ok(())
}

fn cmd_keys_show(storage: &FileStorage) -> Result<(), String> {
    let kp = load_keypair(storage)?;
    println!("Public key  : {}", kp.public_key);
    println!("Fingerprint : {}", kp.fingerprint);
    Ok(())
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn validate_username(username: &str) -> Result<(), String> {
    if username.len() < 3 || username.len() > 32 {
        return Err(format!(
            "Username must be 3–32 characters, got {}",
            username.len()
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_')
    {
        return Err("Username may only contain letters, digits and underscores".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_username_ok() {
        assert!(validate_username("alice").is_ok());
        assert!(validate_username("bob_42").is_ok());
        assert!(validate_username("abc").is_ok());
    }

    #[test]
    fn validate_username_too_short() {
        assert!(validate_username("ab").is_err());
    }

    #[test]
    fn validate_username_too_long() {
        assert!(validate_username(&"a".repeat(33)).is_err());
    }

    #[test]
    fn validate_username_invalid_chars() {
        assert!(validate_username("alice!").is_err());
        assert!(validate_username("alice smith").is_err());
    }

    #[test]
    fn cmd_init_creates_profile() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileStorage::new(dir.path()).unwrap();
        cmd_init(&storage, "testuser", Some("Test User".to_string()), None).unwrap();
        let sp = load_profile(&storage).unwrap();
        assert_eq!(sp.profile.username, "testuser");
        assert_eq!(sp.profile.display_name.as_deref(), Some("Test User"));
    }

    #[test]
    fn cmd_init_rejects_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileStorage::new(dir.path()).unwrap();
        cmd_init(&storage, "alice", None, None).unwrap();
        let result = cmd_init(&storage, "alice", None, None);
        assert!(result.is_err(), "should reject duplicate init");
    }

    #[test]
    fn cmd_post_create_stores_post() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileStorage::new(dir.path()).unwrap();
        cmd_init(&storage, "poster", None, None).unwrap();
        cmd_post_create(&storage, "Hello world", Some("rust,test".to_string()), None).unwrap();
    }

    #[test]
    fn cmd_profile_edit_updates_bio() {
        let dir = tempfile::tempdir().unwrap();
        let storage = FileStorage::new(dir.path()).unwrap();
        cmd_init(&storage, "editor", None, None).unwrap();
        cmd_profile_edit(&storage, None, Some("New bio".to_string())).unwrap();
        let sp = load_profile(&storage).unwrap();
        assert_eq!(sp.profile.bio.as_deref(), Some("New bio"));
    }
}
