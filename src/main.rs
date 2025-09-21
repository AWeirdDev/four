mod nub;
mod calc;

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
    http::{ attachment::Attachment, interaction::{ InteractionResponse, InteractionResponseType } },
};
use twilight_util::builder::{
    embed::{ EmbedBuilder, ImageSource },
    InteractionResponseDataBuilder,
};

use crate::nub::{ get_nubs, NubFinder };

const FOUR_MP3: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/four.mp3"));

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
        &[
            FourCommand::create_command().into(),
            NubCommand::create_command().into(),
            UnzipCommand::create_command().into(),
            YouCommand::create_command().into(),
            OilUpCommand::create_command().into(),
            JumpCommand::create_command().into(),
            SadPhoneCommand::create_command().into(),
            HugeCommand::create_command().into(),
            HelloCommand::create_command().into(),
            RateRagebaitCommand::create_command().into(),
        ]
    ).await?;

    let cache = DefaultInMemoryCache::builder().resource_types(ResourceType::MESSAGE).build();
    let state = Arc::new(AppState::new()?);

    state.nub_finder.commit(get_nubs().await?)?;

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

macro_rules! simple_command_handler {
    ($t:ident, $http:expr, $app_id:expr, $interaction_id:expr, $interaction_token:expr) => {
        $http.interaction($app_id).create_response(
            $interaction_id,
            $interaction_token,
            &(InteractionResponse {
                kind: InteractionResponseType::ChannelMessageWithSource,
                data: Some(
                    InteractionResponseDataBuilder::new()
                        .content($t::URL)
                        .build()
                ),
            })
        ).await?;
    };
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

                                let variant = four.variant.unwrap_or_default();
                                if matches!(variant, FourVariant::Song) {
                                    http.interaction(app_id).create_response(
                                        interaction_id,
                                        &interaction_token,
                                        &(InteractionResponse {
                                            kind: InteractionResponseType::ChannelMessageWithSource,
                                            data: Some(
                                                InteractionResponseDataBuilder::new()
                                                    .attachments([
                                                        Attachment::from_bytes(
                                                            "four.mp3".to_string(),
                                                            FOUR_MP3.to_vec(),
                                                            1
                                                        ),
                                                    ])
                                                    .build()
                                            ),
                                        })
                                    ).await?;
                                } else {
                                    http.interaction(app_id).create_response(
                                        interaction_id,
                                        &interaction_token,
                                        &(InteractionResponse {
                                            kind: InteractionResponseType::ChannelMessageWithSource,
                                            data: Some(
                                                InteractionResponseDataBuilder::new()
                                                    .content(variant.url())
                                                    .build()
                                            ),
                                        })
                                    ).await?;
                                }
                            }

                            "nub" => {
                                let opt = cmd.options.pop().unwrap();

                                match opt.value {
                                    // autocomplete
                                    CommandOptionValue::Focused(data, _) => {
                                        if data.starts_with("nub:") {
                                            http.interaction(app_id).create_response(
                                                interaction_id,
                                                &interaction_token,
                                                &(InteractionResponse {
                                                    kind: InteractionResponseType::ApplicationCommandAutocompleteResult,
                                                    data: Some(
                                                        InteractionResponseDataBuilder::new()
                                                            .choices([
                                                                CommandOptionChoice {
                                                                    name: format!(
                                                                        "shows image: {}",
                                                                        &data
                                                                            .split_once(":")
                                                                            .unwrap().1
                                                                    ),
                                                                    name_localizations: None,
                                                                    value: CommandOptionChoiceValue::String(
                                                                        data
                                                                    ),
                                                                },
                                                            ])
                                                            .build()
                                                    ),
                                                })
                                            ).await?;
                                            return Ok(());
                                        }
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
                                                                .take(25)
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

                            "unzip" => {
                                simple_command_handler!(
                                    UnzipCommand,
                                    http,
                                    app_id,
                                    interaction_id,
                                    &interaction_token
                                );
                            }

                            "you" => {
                                simple_command_handler!(
                                    YouCommand,
                                    http,
                                    app_id,
                                    interaction_id,
                                    &interaction_token
                                );
                            }

                            "oil-up" => {
                                simple_command_handler!(
                                    OilUpCommand,
                                    http,
                                    app_id,
                                    interaction_id,
                                    &interaction_token
                                );
                            }

                            "jump" => {
                                simple_command_handler!(
                                    JumpCommand,
                                    http,
                                    app_id,
                                    interaction_id,
                                    &interaction_token
                                );
                            }

                            "sad-phone" => {
                                simple_command_handler!(
                                    SadPhoneCommand,
                                    http,
                                    app_id,
                                    interaction_id,
                                    &interaction_token
                                );
                            }

                            "huge" => {
                                simple_command_handler!(
                                    HugeCommand,
                                    http,
                                    app_id,
                                    interaction_id,
                                    &interaction_token
                                );
                            }

                            "hello" => {
                                simple_command_handler!(
                                    HelloCommand,
                                    http,
                                    app_id,
                                    interaction_id,
                                    &interaction_token
                                );
                            }

                            "rate-ragebait" => {
                                let rr = match RateRagebaitCommand::from_interaction((*cmd).into()) {
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
                                                .content(rr.rating.url())
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

    #[option(name = "@sillynubnigga", value = "silly-nub-n")]
    SillyNubN,

    #[option(name = "mc freakery", value = "mc-freakery")]
    McFreakery,

    #[option(name = "george washington", value = "george-washington")]
    GeorgeWashington,

    #[option(name = "black boi", value = "black-boi")]
    BlackBoi,

    #[option(name = "+ song", value = "song")]
    Song,
}

impl FourVariant {
    /// Get the URL of this four.
    const fn url(&self) -> &str {
        match self {
            Self::SillyNub =>
                "https://tenor.com/view/nub-nub-cat-silly-cat-silly-kitty-gif-7773816275616110994",
            Self::SillyNubN => "https://aweirddev.github.io/emojis/sillynubn.webp",
            Self::BlackBoi => "https://tenor.com/view/four-gif-26151912",
            Self::GeorgeWashington =>
                "https://tenor.com/view/4-4-aura-4-finger-4-fingers-4-meme-gif-3022035956605345695",
            Self::McFreakery => "https://aweirddev.github.io/emojis/four-mcfreakery.gif",
            Self::Song => unreachable!(),
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

macro_rules! simple_command {
    ($t:ident, name = $name:literal, desc = $desc:literal => $url:expr) => {
        #[derive(CreateCommand, CommandModel)]
        #[command(name = $name, desc = $desc, contexts = "guild bot_dm private_channel")]
        struct $t;

        impl $t {
            const URL: &str = $url;
        }
    };
}

simple_command!(
    UnzipCommand, 
    name = "unzip", 
    desc = "*unzips*"
    => "https://tenor.com/view/sillynubcat-unzips-unzips-nubcat-gif-17883304668920036659"
);

simple_command!(
    YouCommand, 
    name = "you", 
    desc = "you!!!! ye idk"
    => "https://tenor.com/view/silly-cat-nub-cat-nub-idk-i-don%27t-know-gif-6787708826441883618"
);

simple_command!(
    OilUpCommand,
    name = "oil-up",
    desc = "oil up!!!"
    => "https://tenor.com/view/nub-cat-silly-cat-nubcat-sillynubcat-gif-1800738383738753013"
);

simple_command!(
    JumpCommand,
    name = "jump",
    desc = "make the nub cat jump"
    => "https://tenor.com/view/nub-nubcat-jumping-cat-jump-gif-6086329633128237500"
);

simple_command!(
    SadPhoneCommand,
    name = "sad-phone",
    desc = "how can she live a better life while im gone"
    => "https://tenor.com/view/nub-cat-nub-silly-cat-sad-bed-gif-11026855720345092722"
);

simple_command!(
    HugeCommand,
    name = "huge",
    desc = "THIS IS HUGE FOR THE UNEMPLOYED!!!1!!1"
    => "https://tenor.com/view/nub-nub-cat-silly-nub-cat-cat-kitty-gif-12385110001808111979"
);

simple_command!(
    HelloCommand,
    name = "hello",
    desc = "hello fellow nichelings"
    => "https://tenor.com/view/nub-nub-cat-silly-nub-cat-silly-cat-gif-10510046384014080446"
);

#[derive(CreateCommand, CommandModel)]
#[command(
    name = "rate-ragebait",
    desc = "rate this ragebait",
    contexts = "guild bot_dm private_channel"
)]
struct RateRagebaitCommand {
    #[command(desc = "on a scale from 1-10, how'd you rate ts?")]
    rating: RagebaitRating,
}

#[derive(CreateOption, CommandOption)]
enum RagebaitRating {
    #[option(name = "1/10 (retarded)", value = "one")]
    One,

    #[option(name = "7/10 (mid)", value = "seven")]
    Seven,
}

impl RagebaitRating {
    const fn url(&self) -> &str {
        match self {
            Self::One =>
                "https://tenor.com/view/nub-nub-cat-silly-nub-cat-cat-kitty-gif-6600602335070810514",
            Self::Seven =>
                "https://tenor.com/view/nub-nub-cat-silly-nub-cat-cat-kawaii-gif-6031229182476389667",
        }
    }
}
