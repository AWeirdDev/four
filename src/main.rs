use std::{ error::Error, sync::Arc };

use twilight_cache_inmemory::{ DefaultInMemoryCache, ResourceType };
use twilight_gateway::{ Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _ };
use twilight_http::Client as HttpClient;
use twilight_interactions::command::{ CreateCommand, CommandModel };
use twilight_model::{
    application::interaction::InteractionData,
    http::interaction::{ InteractionResponse, InteractionResponseType },
};
use twilight_util::builder::InteractionResponseDataBuilder;

const FOUR: &'static str =
    "https://tenor.com/view/nub-nub-cat-silly-cat-silly-kitty-gif-7773816275616110994";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv_override().ok();

    let token = dotenvy::var("DISCORD_TOKEN")?;

    let intents = Intents::empty();
    let mut shard = Shard::new(ShardId::ONE, token.clone(), intents);

    let http = Arc::new(HttpClient::new(token));

    let interaction_client = {
        let user = http.current_user_application().await?.model().await?;
        http.interaction(user.id)
    };
    interaction_client.set_global_commands(&[FourCommand::create_command().into()]).await?;

    let cache = DefaultInMemoryCache::builder().resource_types(ResourceType::MESSAGE).build();

    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        let Ok(event) = item else {
            tracing::warn!(source = ?item.unwrap_err(), "error receiving event");
            continue;
        };
        cache.update(&event);

        tokio::spawn(handle_event(event, Arc::clone(&http)));
    }

    Ok(())
}

async fn handle_event(
    event: Event,
    http: Arc<HttpClient>
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match event {
        Event::InteractionCreate(ic) => {
            let interaction = ic.0;

            let app_id = interaction.application_id;
            let interaction_id = interaction.id;

            if let Some(data) = interaction.data {
                let interaction_token = interaction.token.as_ref();

                match data {
                    InteractionData::ApplicationCommand(cmd) => {
                        match cmd.name.as_ref() {
                            "four" => {
                                let _four = match FourCommand::from_interaction((*cmd).into()) {
                                    Ok(o) => o,
                                    Err(e) => {
                                        tracing::error!(?e);
                                        return Err(e.into());
                                    }
                                };
                                http.interaction(app_id).create_response(
                                    interaction_id,
                                    &interaction_token,
                                    &(InteractionResponse {
                                        kind: InteractionResponseType::ChannelMessageWithSource,
                                        data: Some(
                                            InteractionResponseDataBuilder::new()
                                                .content(FOUR)
                                                .build()
                                        ),
                                    })
                                ).await?;
                            }
                            _ => (),
                        }
                    }
                    _ => (),
                }
            }
        }
        Event::Ready(_) => {
            println!("Shard is ready");
        }
        _ => {}
    }

    Ok(())
}

#[derive(CreateCommand, CommandModel)]
#[command(name = "four", desc = "ball ball ball")]
struct FourCommand {}
