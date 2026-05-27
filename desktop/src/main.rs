//! SnartNet Desktop — native GUI built with [iced](https://github.com/iced-rs/iced).
//!
//! Architecture follows the Elm model: `App` holds all state; every interaction
//! produces a `Message` that drives a pure `update` function; `view` renders
//! the current state.
//!
//! Current scope: skeleton with three screens — Welcome, Create Profile, and
//! the main Feed. Additional screens will be added incrementally.

use iced::{
    widget::{button, column, container, row, scrollable, text, text_input},
    Alignment, Element, Length, Task,
};
use snartnet_core::{
    KeyPair,
    Post,
    SignedPost,
    Profile,
    SignedProfile,
    FileStorage,
};

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
enum Screen {
    /// App is loading previously stored state.
    #[default]
    Loading,
    /// No identity stored yet — prompt the user to create one.
    Welcome,
    /// Identity creation form.
    CreateProfile {
        username_input: String,
        name_input: String,
        bio_input: String,
        error: Option<String>,
    },
    /// Main application: feed + compose.
    Feed {
        profile: SignedProfile,
        posts: Vec<SignedPost>,
        compose_input: String,
    },
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Message {
    // Loading
    Loaded(Option<(KeyPair, SignedProfile)>),

    // Welcome screen
    GoToCreateProfile,

    // Create profile form
    UsernameChanged(String),
    DisplayNameChanged(String),
    BioChanged(String),
    CreateProfileSubmit,
    ProfileCreated(Result<(KeyPair, SignedProfile), String>),

    // Feed
    ComposeChanged(String),
    PostSubmit,
    PostCreated(Result<SignedPost, String>),
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct App {
    screen: Screen,
    keypair: Option<KeyPair>,
    storage: FileStorage,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let storage = FileStorage::open_default().unwrap_or_else(|e| {
            eprintln!("Warning: could not open default storage: {e}");
            // Fall back to temp dir so the app still opens.
            FileStorage::new(std::env::temp_dir().join("snartnet")).expect("temp storage")
        });

        let app = Self {
            screen: Screen::Loading,
            keypair: None,
            storage,
        };

        let task = Task::perform(load_identity_async(), Message::Loaded);

        (app, task)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // --- Loading ---
            Message::Loaded(result) => {
                match result {
                    Some((kp, sp)) => {
                        let posts = self.load_posts();
                        self.keypair = Some(kp);
                        self.screen = Screen::Feed {
                            profile: sp,
                            posts,
                            compose_input: String::new(),
                        };
                    }
                    None => {
                        self.screen = Screen::Welcome;
                    }
                }
                Task::none()
            }

            // --- Welcome ---
            Message::GoToCreateProfile => {
                self.screen = Screen::CreateProfile {
                    username_input: String::new(),
                    name_input: String::new(),
                    bio_input: String::new(),
                    error: None,
                };
                Task::none()
            }

            // --- Create profile form ---
            Message::UsernameChanged(v) => {
                if let Screen::CreateProfile { ref mut username_input, .. } = self.screen {
                    *username_input = v;
                }
                Task::none()
            }
            Message::DisplayNameChanged(v) => {
                if let Screen::CreateProfile { ref mut name_input, .. } = self.screen {
                    *name_input = v;
                }
                Task::none()
            }
            Message::BioChanged(v) => {
                if let Screen::CreateProfile { ref mut bio_input, .. } = self.screen {
                    *bio_input = v;
                }
                Task::none()
            }
            Message::CreateProfileSubmit => {
                if let Screen::CreateProfile {
                    ref username_input,
                    ref name_input,
                    ref bio_input,
                    ..
                } = self.screen
                {
                    let username = username_input.clone();
                    let name = if name_input.is_empty() { None } else { Some(name_input.clone()) };
                    let bio = if bio_input.is_empty() { None } else { Some(bio_input.clone()) };

                    Task::perform(
                        create_profile_async(username, name, bio),
                        Message::ProfileCreated,
                    )
                } else {
                    Task::none()
                }
            }
            Message::ProfileCreated(result) => {
                match result {
                    Ok((kp, sp)) => {
                        if let Err(e) = self.storage.set_json("keypair", &kp) {
                            eprintln!("Failed to persist keypair: {e}");
                        }
                        if let Err(e) = self.storage.set_json("profile", &sp) {
                            eprintln!("Failed to persist profile: {e}");
                        }
                        self.keypair = Some(kp);
                        self.screen = Screen::Feed {
                            profile: sp,
                            posts: Vec::new(),
                            compose_input: String::new(),
                        };
                    }
                    Err(e) => {
                        if let Screen::CreateProfile { ref mut error, .. } = self.screen {
                            *error = Some(e);
                        }
                    }
                }
                Task::none()
            }

            // --- Feed ---
            Message::ComposeChanged(v) => {
                if let Screen::Feed { ref mut compose_input, .. } = self.screen {
                    *compose_input = v;
                }
                Task::none()
            }
            Message::PostSubmit => {
                if let Screen::Feed {
                    ref profile,
                    ref compose_input,
                    ..
                } = self.screen
                {
                    if compose_input.is_empty() {
                        return Task::none();
                    }
                    let fingerprint = profile.profile.fingerprint.clone();
                    let content = compose_input.clone();
                    let kp = self.keypair.clone();

                    Task::perform(
                        create_post_async(fingerprint, content, kp),
                        Message::PostCreated,
                    )
                } else {
                    Task::none()
                }
            }
            Message::PostCreated(result) => {
                if let Ok(ref sp) = result {
                    let key = format!("post_{}", sp.post.id);
                    if let Err(e) = self.storage.set_json(&key, sp) {
                        eprintln!("Failed to persist post: {e}");
                    }
                }
                if let Screen::Feed { ref mut posts, ref mut compose_input, .. } = self.screen {
                    if let Ok(sp) = result {
                        posts.insert(0, sp);
                        compose_input.clear();
                    }
                }
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match &self.screen {
            Screen::Loading => view_loading(),
            Screen::Welcome => view_welcome(),
            Screen::CreateProfile { username_input, name_input, bio_input, error } => {
                view_create_profile(username_input, name_input, bio_input, error.as_deref())
            }
            Screen::Feed { profile, posts, compose_input } => {
                view_feed(profile, posts, compose_input)
            }
        }
    }

    fn load_posts(&self) -> Vec<SignedPost> {
        // Attempt to read all stored posts; non-critical — return empty on error.
        // Real implementation will enumerate the storage directory.
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

fn view_loading<'a>() -> Element<'a, Message> {
    container(text("Loading…").size(24))
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .into()
}

fn view_welcome<'a>() -> Element<'a, Message> {
    let content = column![
        text("Welcome to SnartNet").size(32),
        text("Own your identity. Control your data. Connect directly.").size(16),
        button("Create your identity").on_press(Message::GoToCreateProfile),
    ]
    .spacing(16)
    .align_x(Alignment::Center);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .into()
}

fn view_create_profile<'a>(
    username: &'a str,
    name: &'a str,
    bio: &'a str,
    error: Option<&'a str>,
) -> Element<'a, Message> {
    let mut form = column![
        text("Create profile").size(28),
        text_input("Username (required)", username).on_input(Message::UsernameChanged),
        text_input("Display name (optional)", name).on_input(Message::DisplayNameChanged),
        text_input("Bio (optional)", bio).on_input(Message::BioChanged),
        button("Create").on_press(Message::CreateProfileSubmit),
    ]
    .spacing(12)
    .padding(24)
    .max_width(480);

    if let Some(err) = error {
        form = form.push(text(format!("⚠ {err}")).size(14));
    }

    container(form)
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .into()
}

fn view_feed<'a>(
    profile: &'a SignedProfile,
    posts: &'a [SignedPost],
    compose: &'a str,
) -> Element<'a, Message> {
    // Sidebar
    let sidebar = column![
        text(format!("@{}", profile.profile.username)).size(20),
        text(
            profile
                .profile
                .display_name
                .as_deref()
                .unwrap_or("")
        )
        .size(14),
    ]
    .spacing(8)
    .padding(16)
    .width(200);

    // Compose
    let compose_area = column![
        text_input("What's on your mind?", compose).on_input(Message::ComposeChanged),
        button("Post").on_press(Message::PostSubmit),
    ]
    .spacing(8)
    .padding(16);

    // Post list
    let post_list: Element<Message> = if posts.is_empty() {
        text("No posts yet — create your first one above!").size(14).into()
    } else {
        let items: Vec<Element<Message>> = posts
            .iter()
            .map(|sp| {
                column![
                    text(&sp.post.content).size(16),
                    text(
                        sp.post
                            .created_at
                            .format("%Y-%m-%d %H:%M UTC")
                            .to_string()
                    )
                    .size(12),
                ]
                .spacing(4)
                .padding(12)
                .into()
            })
            .collect();

        scrollable(column(items).spacing(8)).into()
    };

    let main_col = column![compose_area, post_list]
        .spacing(8)
        .width(Length::Fill);

    row![sidebar, main_col].spacing(0).into()
}

// ---------------------------------------------------------------------------
// Async helpers (run on the iced executor)
// ---------------------------------------------------------------------------

async fn load_identity_async() -> Option<(KeyPair, SignedProfile)> {
    let storage = FileStorage::open_default().ok()?;
    let kp = storage.get_json::<KeyPair>("keypair").ok()??;
    let sp = storage.get_json::<SignedProfile>("profile").ok()??;
    Some((kp, sp))
}

async fn create_profile_async(
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
) -> Result<(KeyPair, SignedProfile), String> {
    // Basic username validation
    if username.len() < 3 || username.len() > 32 {
        return Err(format!(
            "Username must be 3–32 characters (got {})",
            username.len()
        ));
    }
    if !username.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("Username may only contain letters, digits and underscores".to_string());
    }

    let kp = KeyPair::generate()?;
    let key_info = kp.get_public_info();
    let mut profile = Profile::new(username, key_info);
    if display_name.is_some() || bio.is_some() {
        profile.update(display_name, bio);
    }
    let mut signed = SignedProfile::create(profile, &kp)?;
    signed.profile.magnet_uri = Some(signed.profile.generate_magnet_uri());

    Ok((kp, signed))
}

async fn create_post_async(
    author_fingerprint: String,
    content: String,
    keypair: Option<KeyPair>,
) -> Result<SignedPost, String> {
    let kp = keypair.ok_or("No keypair available")?;
    let post = Post::new(author_fingerprint, content, None, None);
    SignedPost::create(post, &kp)
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> iced::Result {
    iced::application("SnartNet", App::update, App::view)
        .window_size((1024.0, 700.0))
        .run_with(App::new)
}
