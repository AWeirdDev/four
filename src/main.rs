mod nub;

use std::{ error::Error, sync::Arc };

use twilight_cache_inmemory::{ DefaultInMemoryCache, ResourceType };
use twilight_gateway::{ Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _ };
use twilight_http::Client as HttpClient;
use twilight_interactions::command::{ CommandModel, CommandOption, CreateCommand, CreateOption };
use twilight_model::{
    application::{
        command::{ CommandOptionChoice, CommandOptionChoiceValue },
        interaction::{ application_command::CommandOptionValue, InteractionData },
    },
    http::interaction::{ InteractionResponse, InteractionResponseType },
};
use twilight_util::builder::{
    embed::{ EmbedBuilder, ImageSource },
    InteractionResponseDataBuilder,
};

use crate::nub::{ fetch_nubs, get_nubs, save_nubs, NubFinder };

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

    interaction_client.set_global_commands(
        &[FourCommand::create_command().into(), NubCommand::create_command().into()]
    ).await?;

    let cache = DefaultInMemoryCache::builder().resource_types(ResourceType::MESSAGE).build();
    let state = Arc::new(AppState::new()?);

    tokio::spawn(auto_update_nubs(state.clone()));

    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        let Ok(event) = item else {
            tracing::warn!(source = ?item.unwrap_err(), "error receiving event");
            continue;
        };
        cache.update(&event);

        tokio::spawn(handle_event(event, http.clone(), state.clone()));
    }

    Ok(())
}

async fn handle_event(
    event: Event,
    http: Arc<HttpClient>,
    state: Arc<AppState>
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match event {
        Event::InteractionCreate(ic) => {
            let interaction = ic.0;

            let app_id = interaction.application_id;
            let interaction_id = interaction.id;

            if let Some(data) = interaction.data {
                let interaction_token = interaction.token.as_ref();

                match data {
                    InteractionData::ApplicationCommand(mut cmd) => {
                        match cmd.name.as_ref() {
                            "four" => {
                                let four = match FourCommand::from_interaction((*cmd).into()) {
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
                                                .content(four.variant.unwrap_or_default().url())
                                                .build()
                                        ),
                                    })
                                ).await?;
                            }

                            "nub" => {
                                let opt = cmd.options.pop().unwrap();

                                match opt.value {
                                    // autocomplete
                                    CommandOptionValue::Focused(data, _) => {
                                        let results = tokio::task::spawn_blocking(move || {
                                            state.nub_finder.search(&data)
                                        }).await??;
                                        http.interaction(app_id).create_response(
                                            interaction_id,
                                            &interaction_token,
                                            &(InteractionResponse {
                                                kind: InteractionResponseType::ApplicationCommandAutocompleteResult,
                                                data: Some(
                                                    InteractionResponseDataBuilder::new()
                                                        .choices(
                                                            results
                                                                .into_iter()
                                                                .map(
                                                                    |(
                                                                        url,
                                                                        keyword,
                                                                    )| CommandOptionChoice {
                                                                        name: keyword,
                                                                        name_localizations: None,
                                                                        value: CommandOptionChoiceValue::String(
                                                                            format!("nub:{}", url)
                                                                        ),
                                                                    }
                                                                )
                                                        )
                                                        .build()
                                                ),
                                            })
                                        ).await?;
                                    }

                                    // finalized option
                                    CommandOptionValue::String(data) => {
                                        let url = {
                                            if data.starts_with("nub:") {
                                                data.split_once(':').unwrap().1.to_string()
                                            } else {
                                                let results = tokio::task::spawn_blocking(move || {
                                                    state.nub_finder.search(&data)
                                                }).await??;
                                                if let Some((url, _)) = results.first() {
                                                    url.to_string()
                                                } else {
                                                    http.interaction(app_id).create_response(
                                                        interaction_id,
                                                        &interaction_token,
                                                        &(InteractionResponse {
                                                            kind: InteractionResponseType::ChannelMessageWithSource,
                                                            data: Some(
                                                                InteractionResponseDataBuilder::new()
                                                                    .content(
                                                                        "i couldn't find that nub :("
                                                                    )
                                                                    .build()
                                                            ),
                                                        })
                                                    ).await?;
                                                    return Ok(());
                                                }
                                            }
                                        };

                                        http.interaction(app_id).create_response(
                                            interaction_id,
                                            &interaction_token,
                                            &(InteractionResponse {
                                                kind: InteractionResponseType::ChannelMessageWithSource,
                                                data: Some(
                                                    InteractionResponseDataBuilder::new()
                                                        .embeds([
                                                            EmbedBuilder::new()
                                                                .image(ImageSource::url(url)?)
                                                                .build(),
                                                        ])
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

async fn update_nubs(
    state: Arc<AppState>
) -> Result<(), Box<dyn core::error::Error + Send + Sync>> {
    let nubs = fetch_nubs().await?;
    save_nubs(&nubs)?;
    tokio::task::spawn_blocking(move || { state.nub_finder.commit(nubs) }).await??;

    Ok(())
}

async fn auto_update_nubs(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(core::time::Duration::from_secs(300));

    interval.tick().await;
    state.nub_finder.commit(get_nubs().await.unwrap()).unwrap();

    loop {
        interval.tick().await;
        if let Err(e) = update_nubs(state.clone()).await {
            tracing::error!(?e, "failed to update nubs");
        };
    }
}

#[repr(transparent)]
struct AppState {
    nub_finder: NubFinder,
}

impl AppState {
    fn new() -> Result<Self, Box<dyn core::error::Error + Send + Sync>> {
        Ok(Self { nub_finder: NubFinder::new()? })
    }
}

#[derive(CreateCommand, CommandModel)]
#[command(name = "four", desc = "ball ball ball", contexts = "guild bot_dm private_channel")]
struct FourCommand {
    #[command(desc = "the variant to choose from. of course you love sillynubcat wdym")]
    variant: Option<FourVariant>,
}

#[derive(CreateOption, CommandOption)]
enum FourVariant {
    #[option(name = "silly nub cat", value = "silly-nub")]
    SillyNub,

    #[option(name = "black boi", value = "black-boi")]
    BlackBoi,
}

impl FourVariant {
    /// Get the URL of this four.
    const fn url(&self) -> &str {
        match self {
            Self::SillyNub =>
                "https://tenor.com/view/nub-nub-cat-silly-cat-silly-kitty-gif-7773816275616110994",
            Self::BlackBoi =>
                "https://content.imageresizer.com/images/memes/Black-Boi-holding-up-4-fingers-meme-6.jpg",
        }
    }
}

impl Default for FourVariant {
    fn default() -> Self {
        Self::SillyNub
    }
}

#[derive(CreateCommand, CommandModel)]
#[command(name = "nub", desc = "find a nub cat", contexts = "guild bot_dm private_channel")]
struct NubCommand {
    #[command(desc = "the nub cat you're looking for", autocomplete = true)]
    #[allow(unused)]
    query: String,
}
