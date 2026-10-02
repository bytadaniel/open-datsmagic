//! Сессионный лидерборд, атомарно публикуемый в JSON для локального визуализатора.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tokio::{
    task::JoinHandle,
    time::{self, Duration},
};

use crate::engine::{state::WorldSnapshot, GameEngine};

const WORD_A: [&str; 32] = [
    "Amber", "Azure", "Bold", "Bright", "Calm", "Clever", "Copper", "Cosmic", "Crimson", "Daring",
    "Dusk", "Emerald", "Frost", "Golden", "Hidden", "Ivory", "Lucky", "Mellow", "Misty", "Neon",
    "Noble", "Rapid", "Quiet", "Royal", "Rustic", "Silver", "Solar", "Swift", "Velvet", "Wild",
    "Wise", "Zesty",
];
const WORD_B: [&str; 32] = [
    "Autumn",
    "Breezy",
    "Celestial",
    "Distant",
    "Electric",
    "Fearless",
    "Gentle",
    "Glacial",
    "Jade",
    "Keen",
    "Lunar",
    "Marble",
    "Mighty",
    "Northern",
    "Opal",
    "Patient",
    "Primal",
    "Radiant",
    "Restless",
    "Scarlet",
    "Shining",
    "Silent",
    "Smoky",
    "Stellar",
    "Thunder",
    "Umbral",
    "Verdant",
    "Wandering",
    "Winter",
    "Yellow",
    "Zealous",
    "Zenith",
];
const WORD_C: [&str; 32] = [
    "Badger",
    "Comet",
    "Crane",
    "Drifter",
    "Falcon",
    "Fox",
    "Heron",
    "Jackal",
    "Kestrel",
    "Lantern",
    "Mantis",
    "Meteor",
    "Otter",
    "Panther",
    "Phoenix",
    "Pioneer",
    "Raven",
    "Rider",
    "Sailor",
    "Scarab",
    "Sentinel",
    "Shark",
    "Sparrow",
    "Stargazer",
    "Tiger",
    "Voyager",
    "Wolf",
    "Wren",
    "Puffin",
    "Orchid",
    "Harrier",
    "Nomad",
];
const WORD_D: [&str; 32] = [
    "Anvil", "Arrow", "Beacon", "Bloom", "Canyon", "Cipher", "Clover", "Current", "Dynamo",
    "Ember", "Engine", "Feather", "Flame", "Garden", "Hammer", "Harbor", "Horizon", "Kernel",
    "Meadow", "Mirage", "Monolith", "Orbit", "Quartz", "River", "Rocket", "Signal", "Summit",
    "Talisman", "Tempest", "Thistle", "Turbine", "Vortex",
];

fn fnv1a_seed(token: &str, attempt: u64) -> u64 {
    let hash = token
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    hash.wrapping_add(attempt.wrapping_mul(0x9e3779b97f4a7c15))
}

fn next_word_index(state: &mut u64) -> usize {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((value ^ (value >> 31)) as usize) & 31
}

fn candidate_team_name(token: &str, attempt: u64) -> String {
    let mut state = fnv1a_seed(token, attempt);
    format!(
        "{}-{}-{}-{}",
        WORD_A[next_word_index(&mut state)],
        WORD_B[next_word_index(&mut state)],
        WORD_C[next_word_index(&mut state)],
        WORD_D[next_word_index(&mut state)],
    )
}

fn unique_team_name(token: &str, used_names: &mut HashSet<String>) -> String {
    for attempt in 0..(WORD_A.len() * WORD_B.len() * WORD_C.len() * WORD_D.len()) as u64 {
        let candidate = candidate_team_name(token, attempt);
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("team name dictionary exhausted")
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LeaderboardFile {
    pub generated_at_unix_ms: u128,
    pub updated_at_unix_ms: u128,
    pub teams: Vec<LeaderboardEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaderboardEntry {
    pub rank: usize,
    pub team: String,
    /// Непрозрачный стабильный идентификатор токена для доверенного агрегатора.
    /// Исходный токен в session report не сериализуется.
    pub team_id: String,
    pub gold: u64,
    pub gold_collected: u64,
    pub carpets_lost: u64,
    pub distance_travelled: f64,
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

fn make_file(
    snapshot: &WorldSnapshot,
    generated_at_unix_ms: u128,
    team_names: &mut HashMap<String, String>,
    used_names: &mut HashSet<String>,
) -> LeaderboardFile {
    let mut player_ids = snapshot.world.players.keys().cloned().collect::<Vec<_>>();
    player_ids.sort();
    let mut teams = Vec::with_capacity(player_ids.len());

    for player_id in player_ids {
        let team = team_names
            .entry(player_id.clone())
            .or_insert_with(|| unique_team_name(&player_id, used_names))
            .clone();
        let Some(player) = snapshot.world.players.get(&player_id) else {
            continue;
        };
        let gold_collected = player.gold_collected_total.max(
            player
                .carpets
                .values()
                .map(|carpet| carpet.gold_collected_total)
                .sum(),
        );
        let carpets_lost = player
            .carpets
            .values()
            .map(|carpet| u64::from(carpet.death_count))
            .sum();
        teams.push(LeaderboardEntry {
            rank: 0,
            team,
            // Player IDs are token fingerprints; raw tokens are never retained in game state.
            team_id: player_id.clone(),
            gold: u64::from(player.score),
            gold_collected,
            carpets_lost,
            distance_travelled: player.distance_travelled,
        });
    }

    teams.sort_by(|a, b| {
        b.gold
            .cmp(&a.gold)
            .then_with(|| b.gold_collected.cmp(&a.gold_collected))
            .then_with(|| a.team.cmp(&b.team))
    });
    for (index, team) in teams.iter_mut().enumerate() {
        team.rank = index + 1;
    }

    LeaderboardFile {
        generated_at_unix_ms,
        updated_at_unix_ms: now_unix_ms(),
        teams,
    }
}

async fn write_atomic(
    path: &Path,
    file: &LeaderboardFile,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    let bytes = serde_json::to_vec_pretty(file)?;
    let mut temporary = PathBuf::from(path);
    temporary.set_extension("json.tmp");
    tokio::fs::write(&temporary, bytes).await?;
    tokio::fs::rename(&temporary, path).await?;
    Ok(())
}

/// Truncates the prior session file and begins publishing current session stats once per second.
pub async fn spawn(
    engine: GameEngine,
    path: PathBuf,
) -> Result<JoinHandle<()>, Box<dyn std::error::Error + Send + Sync>> {
    let generated_at_unix_ms = now_unix_ms();
    write_atomic(
        &path,
        &LeaderboardFile {
            generated_at_unix_ms,
            updated_at_unix_ms: generated_at_unix_ms,
            teams: Vec::new(),
        },
    )
    .await?;

    Ok(tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(1));
        let mut team_names = HashMap::new();
        let mut used_names = HashSet::new();
        loop {
            interval.tick().await;
            let snapshot = engine.get_snapshot();
            let file = make_file(
                &snapshot,
                generated_at_unix_ms,
                &mut team_names,
                &mut used_names,
            );
            if let Err(error) = write_atomic(&path, &file).await {
                tracing::error!(path = %path.display(), %error, "Не удалось обновить leaderboard.json");
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::state::{CarpetState, GameState};

    fn fingerprint(token: &str) -> String {
        let hash = token
            .as_bytes()
            .iter()
            .fold(0xcbf29ce484222325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
            });
        format!("{hash:016x}")
    }

    #[test]
    fn leaderboard_sorts_teams_and_never_serializes_auth_tokens() {
        let mut state = GameState::new();
        let first_id = fingerprint("secret-alpha");
        let first_carpet = format!("{first_id}_0");
        let mut first = crate::engine::state::PlayerState::new(first_id, 0.0, 0.0, 40.0, 110.0);
        first.score = 70;
        first.gold_collected_total = 100;
        first.distance_travelled = 123.5;
        first.carpets.insert(
            first_carpet.clone(),
            CarpetState::new(&first_carpet, 0.0, 0.0, 40.0, 110.0),
        );
        first.carpets.get_mut(&first_carpet).unwrap().death_count = 2;
        state.world.players.insert(first.id.clone(), first);
        let second_id = fingerprint("secret-beta");
        let mut second = crate::engine::state::PlayerState::new(second_id, 0.0, 0.0, 40.0, 110.0);
        second.score = 120;
        state.world.players.insert(second.id.clone(), second);

        let mut names = HashMap::new();
        let mut used_names = HashSet::new();
        let file = make_file(&state.to_snapshot(), 1, &mut names, &mut used_names);
        assert_eq!(file.teams[0].gold, 120);
        assert_eq!(file.teams[1].gold_collected, 100);
        assert_eq!(file.teams[1].carpets_lost, 2);
        let json = serde_json::to_string(&file).unwrap();
        assert!(!json.contains("secret-alpha"));
        assert!(!json.contains("secret-beta"));
        assert!(file.teams[0].team.contains('-'));
    }

    #[test]
    fn ten_thousand_tokens_receive_stable_unique_word_names() {
        let mut used_names = HashSet::new();
        let mut names = HashMap::new();
        for index in 0..10_000 {
            let token = format!("competitor-token-{index}");
            names.insert(token.clone(), unique_team_name(&token, &mut used_names));
        }
        assert_eq!(used_names.len(), 10_000);
        assert_eq!(
            names["competitor-token-314"],
            candidate_team_name("competitor-token-314", 0)
        );
    }
}
