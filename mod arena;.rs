mod arena;
mod move_unit;

use futures::future::join_all;
use std::time::Duration;
use reqwest::{StatusCode, Client};

// Константы
const RPS_DELAY_MS: u64 = 160;
const MAX_REGISTRATION_ATTEMPTS: usize = 5;
const BASE_RETRY_DELAY_SEC: u64 = 1;
const ARENA_UPDATE_DELAY_SEC: u64 = 2;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    
    // Регистрация
    if let Err(e) = register_player(&client).await {
        eprintln!("Failed to register: {}", e);
        return Err(e);
    }

    // Основной игровой цикл
    game_loop(client).await?;

    Ok(())
}

async fn game_loop(client: Client) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        // Получение состояния арены
        let arena_state = match arena::get_arena_info(&client).await {
            Ok(state) => state,
            Err(e) => {
                eprintln!("Arena error: {}", e);
                tokio::time::sleep(Duration::from_secs(ARENA_UPDATE_DELAY_SEC)).await;
                continue;
            }
        };

        // Обработка муравьев
        if !arena_state.ants.is_empty() {
            process_ants(&client, &arena_state).await;
        }

        // Ожидание следующего тика
        let wait_time = arena_state.tick_time / 1000.0;
        tokio::time::sleep(Duration::from_secs_f32(wait_time)).await;
    }
}

async fn process_ants(client: &Client, arena_state: &ArenaState) {
    let ants: Vec<_> = arena_state.ants.values().collect();
    let mut tasks = Vec::with_capacity(ants.len());

    for ant in ants {
        let client = client.clone();
        let arena = arena_state.clone();
        let ant = ant.clone();

        tasks.push(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(RPS_DELAY_MS)).await;
            match move_unit::move_ant(&client, &arena, &ant).await {
                Ok(_) => println!("Ant {} moved", ant.id),
                Err(e) => eprintln!("Ant {} error: {}", ant.id, e),
            }
        }));
    }

    join_all(tasks).await;
}

async fn register_player(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let token = std::env::var("API_TOKEN")?;
    let url = "https://games-test.datsteam.dev/api/register";

    for attempt in 0..MAX_REGISTRATION_ATTEMPTS {
        let response = client
            .post(url)
            .header("accept", "application/json")
            .header("X-Auth-Token", &token)
            .send()
            .await?;

        if response.status().is_success() {
            println!("Registration successful");
            return Ok(());
        }

        let delay = BASE_RETRY_DELAY_SEC * 2u64.pow(attempt as u32);
        tokio::time::sleep(Duration::from_secs(delay)).await;
    }

    Err("Max registration attempts reached".into())
}