

```rust
mod arena;
mod move_unit;

use futures::future::join_all;
use std::time::Duration;
use reqwest::StatusCode;

// Константы
const RPS_DELAY_MS: u64 = 160;
const REGISTRATION_RETRIES: usize = 5;
const BASE_RETRY_DELAY: u64 = 1;
const ARENA_UPDATE_DELAY: u64 = 2;

#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();
    let mut registered = false;

    loop {
        if !registered {
            if let Err(e) = try_register(&client, REGISTRATION_RETRIES).await {
                eprintln!("Critical: {}", e);
                return;
            }
            registered = true;
        }

        let arena_state = match arena::get_arena_info(&client).await {
            Ok(state) => state,
            Err(e) => {
                eprintln!("Ошибка получения арены: {}", e);
                tokio::time::sleep(Duration::from_secs(ARENA_UPDATE_DELAY)).await;
                continue;
            }
        };

        let ants = arena_state.ants.values().collect::<Vec<_>>();
        if ants.is_empty() {
            let wait_time = arena_state.tick_time / 1000.0;
            tokio::time::sleep(Duration::from_secs_f32(wait_time)).await;
            continue;
        }

        // Параллельное выполнение с контролем RPS
        let mut tasks = Vec::new();
        for ant in ants {
            let client = client.clone();
            let arena = arena_state.clone();
            let ant = ant.clone();
            
            tasks.push(tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(RPS_DELAY_MS)).await;
                move_unit::move_ant(&client, &arena, &ant).await
            }));
        }

        let results = join_all(tasks).await;
        for result in results {
            match result {
                Ok(Ok(_)) => (),
                Ok(Err(e)) => eprintln!("Ошибка муравья: {}", e),
                Err(e) => eprintln!("Ошибка задачи: {}", e),
            }
        }

        let wait_time = arena_state.tick_time / 1000.0;
        tokio::time::sleep(Duration::from_secs_f32(wait_time)).await;
    }
}

async fn try_register(client: &reqwest::Client, max_retries: usize) -> Result<(), String> {
    let token = std::env::var("API_TOKEN").map_err(|_| "API_TOKEN not set".to_string())?;
    let url = "https://games-test.datsteam.dev/api/register";

    for attempt in 0..max_retries {
        match client
            .post(url)
            .header("accept", "application/json")
            .header("X-Auth-Token", &token)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                eprintln!("Ошибка регистрации ({}): {}", status, text);
            }
            Err(e) => eprintln!("Ошибка запроса: {}", e),
        }

        let delay = BASE_RETRY_DELAY * 2u64.pow(attempt as u32);
        tokio::time::sleep(Duration::from_secs(delay)).await;
    }

    Err(format!("Не удалось зарегистрироваться после {} попыток", max_retries))
}