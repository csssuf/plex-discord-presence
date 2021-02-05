use std::io::{self, Read};
use std::mem;
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Duration;

use discord_game_sdk::{Activity, Discord};

const CLIENT_ID: i64 = 807024921858277376;

#[derive(Debug, Clone)]
enum PlaybackChange {
    Started(String),
    Stopped,
}

fn init_discord() -> Result<Discord<'static, ()>, Box<dyn std::error::Error>> {
    let mut discord = Discord::new(CLIENT_ID)?;
    *discord.event_handler_mut() = Some(());
    Ok(discord)
}

fn discord_update_loop(rx: Receiver<PlaybackChange>) -> Result<(), Box<dyn std::error::Error>> {
    let mut discord = init_discord()?;

    loop {
        if let Ok(change) = rx.recv_timeout(Duration::from_millis(1000)) {
            match change {
                PlaybackChange::Started(contents) => discord.update_activity(
                    &Activity::empty().with_state(&contents),
                    |_, e| { eprintln!("result: {:?}", e) }
                ),
                PlaybackChange::Stopped => {
                    discord.clear_activity(|_, __| {});
                    discord.run_callbacks()?;

                    // Just calling clear_activity() by itself leaves "Playing Plex" still in
                    // the presence, so we do this drop-and-recreate hack to trick Discord into
                    // clearing the presence entirely.
                    mem::drop(discord);
                    discord = init_discord()?;
                },
            }
        }

        discord.run_callbacks()?;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = channel::<PlaybackChange>();

    thread::spawn(move || { discord_update_loop(rx).unwrap() });

    loop {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        if buf.trim() == "clear" {
            tx.send(PlaybackChange::Stopped)?;
        } else {
            tx.send(PlaybackChange::Started(buf))?;
        }
    }
}
