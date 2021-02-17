use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Duration;

use discord_game_sdk::{Activity, Discord};
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
                    |_, e| eprintln!("result: {:?}", e),
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

fn extract_trackinfo(sessions: Vec<&SessionMetadata>) -> Option<TrackInfo> {
    match sessions.len() {
        0 => None,
        _ => {
            let metadata = &sessions[0].metadata;

            let artist = metadata
                .original_title
                .clone()
                .or(metadata.grandparent_title.clone());

            Some(TrackInfo {
                title: metadata.title.clone(),
                album: metadata
                    .parent_title
                    .clone()
                    .unwrap_or_else(|| String::new()),
                artist: artist.unwrap_or_else(|| String::new()),
            })
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config =
        config::load_config()?.ok_or_else(|| format!("Please edit the above config and rerun."))?;

    let (tx, rx) = channel::<PlaybackChange>();

    let discord_interval = config.discord.update_interval_ms;
    thread::spawn(move || discord_update_loop(rx, discord_interval).unwrap());

    let acct = MyPlexAccount::login(&config.plex.username, &config.plex.password).await?;
    let devices = acct.get_devices().await?;
    let filtered = devices
        .iter()
        .filter(|d| d.get_name() == &config.plex.server_name)
        .collect::<Vec<_>>();
    let server = filtered[0].connect_to_server().await?;

    let mut playing = false;

    loop {
        let sessions = server.get_sessions().await;

        match sessions {
            Ok(s) => {
                let valid_sessions = s
                    .metadata
                    .iter()
                    .filter(|m| m.metadata.media_type == MediaType::Track)
                    .filter(|m| m.player.state == "playing")
                    .collect::<Vec<_>>();

                let this_track = extract_trackinfo(valid_sessions);
                println!("{:#?}", this_track);
                if let Some(track) = this_track {
                    playing = true;
                    tx.send(PlaybackChange::Started(track)).unwrap();
                } else if playing && this_track.is_none() {
                    playing = false;
                    tx.send(PlaybackChange::Stopped).unwrap();
                }
            }
            Err(e) => println!("Failed to fetch sessions: {:?}", e),
        }

        thread::sleep(Duration::from_millis(config.plex.polling_interval_ms));
    }
}
