use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Duration;

use discord_game_sdk::{Activity, Discord};
use env_logger::Env;
use log::{debug, error, info, warn};
use plex_api::{MediaType, MyPlexAccount, SessionMetadata};

mod config;

const CLIENT_ID: i64 = 807024921858277376;

#[derive(Debug, Clone)]
enum PlaybackChange {
    Started(TrackInfo),
    Stopped,
}

#[derive(Debug, Clone)]
struct TrackInfo {
    title: String,
    album: String,
    artist: String,
}

fn init_plex_client() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(package) = option_env!("CARGO_PKG_NAME") {
        let mut product = plex_api::X_PLEX_PRODUCT.write()?;
        *product = package;
    }

    if let Some(package_version) = option_env!("CARGO_PKG_VERSION") {
        let mut version = plex_api::X_PLEX_VERSION.write()?;
        *version = package_version;
    }

    let mut ident = plex_api::X_PLEX_CLIENT_IDENTIFIER.write()?;
    match sys_info::hostname() {
        Ok(hostname) => *ident = format!("{}_{}", "plex-discord-presence", hostname),
        Err(e) => warn!("Could not get hostname for X-Plex-Client-Identifier: {}", e),
    }

    Ok(())
}

fn init_discord() -> Result<Discord<'static, ()>, Box<dyn std::error::Error>> {
    let mut discord = Discord::new(CLIENT_ID)?;
    *discord.event_handler_mut() = Some(());
    Ok(discord)
}

fn discord_update_loop(
    rx: Receiver<PlaybackChange>,
    interval_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut discord = init_discord()?;

    loop {
        if let Ok(change) = rx.recv_timeout(Duration::from_millis(interval_ms)) {
            match change {
                PlaybackChange::Started(contents) => discord.update_activity(
                    &Activity::empty()
                        .with_state(&format!("Track: {}", contents.title))
                        .with_details(&format!("Artist: {}", contents.artist)),
                    |_, e| {
                        if e.is_err() {
                            error!("Discord callback failed: {:?}", e)
                        }
                    },
                ),
                PlaybackChange::Stopped => {
                    discord.clear_activity(|_, __| {});
                    discord.run_callbacks()?;
                    thread::sleep(Duration::from_millis(1000));

                    // Just calling clear_activity() by itself leaves "Playing Plex" still in
                    // the presence, so we do this drop-and-recreate hack to trick Discord into
                    // clearing the presence entirely.
                    discord = init_discord()?;
                }
            }
        }

        discord.run_callbacks()?;
    }
}

fn extract_trackinfo(sessions: &[SessionMetadata]) -> Option<TrackInfo> {
    sessions
        .into_iter()
        .filter(|m| m.metadata.media_type == MediaType::Track)
        .filter(|m| m.player.state == "playing")
        .next()
        .map(|session| {
            let artist = session
                .metadata
                .original_title
                .clone()
                .or(session.metadata.grandparent_title.clone());

            TrackInfo {
                title: session.metadata.title.clone(),
                album: session
                    .metadata
                    .parent_title
                    .clone()
                    .unwrap_or_else(|| String::new()),
                artist: artist.unwrap_or_else(|| String::new()),
            }
        })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(
        Env::default().default_filter_or("info,plex_api::media_container::media::stream=error"),
    )
    .init();

    init_plex_client()?;

    let config =
        config::load_config()?.ok_or_else(|| format!("Please edit the above config and rerun."))?;

    let (tx, rx) = channel::<PlaybackChange>();

    let discord_interval = config.discord.update_interval_ms;
    thread::spawn(move || discord_update_loop(rx, discord_interval).unwrap());

    let acct = MyPlexAccount::login(&config.plex.username, &config.plex.password).await?;
    info!("Logged in to Plex");
    let devices = acct.get_devices().await?;
    info!("Fetched all devices");
    let filtered = devices
        .iter()
        .filter(|d| d.get_name() == &config.plex.server_name)
        .collect::<Vec<_>>();
    let server = filtered[0].connect_to_server().await?;
    info!(
        "Connection to server {} established",
        &config.plex.server_name
    );

    let mut playing = false;

    loop {
        let sessions = server.get_sessions().await;

        match sessions {
            Ok(s) => {
                let this_track = extract_trackinfo(&s.metadata);
                debug!("Track: {:#?}", this_track);
                if let Some(track) = this_track {
                    if !playing {
                        info!(
                            "Now playing: {} by {} on {}",
                            track.title, track.artist, track.album
                        );
                    }
                    playing = true;
                    tx.send(PlaybackChange::Started(track)).unwrap();
                } else if playing && this_track.is_none() {
                    playing = false;
                    info!("Playback stopped");
                    tx.send(PlaybackChange::Stopped).unwrap();
                }
            }
            Err(e) => error!("Failed to fetch sessions: {:?}", e),
        }

        thread::sleep(Duration::from_millis(config.plex.polling_interval_ms));
    }
}
