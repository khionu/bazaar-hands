// Standard lib
use std::env;
use std::sync::LazyLock;
// Basic utilities
use bytes::Bytes;
use ed25519_dalek::{Signature, Verifier, VerifyingKey, PUBLIC_KEY_LENGTH};
use hex::FromHex;
use tracing::{debug, error, info, Level};
// HTTP
use salvo::{
  http::Method,
  prelude::*
};
use http_body_util::BodyExt;
// Discord
use twilight_http::Client;
use twilight_model::{
  application::{
    interaction::{Interaction, InteractionType},
  },
  channel::message::MessageFlags,
  http::interaction::{InteractionResponse, InteractionResponseType},
  id::{Id, marker::{ApplicationMarker, GuildMarker}},
};
use twilight_util::builder::{
  embed::EmbedBuilder,
  InteractionResponseDataBuilder,
};

mod commands;

pub static APP_ID: LazyLock<Id<ApplicationMarker>> = LazyLock::new(|| {
  let val = env::var("BAZAAR_HAND_APP_ID")
    .expect("Missing BAZAAR_HAND_APP_ID environment variable");
  Id::new(val.parse().expect("BAZAAR_HAND_APP_ID is invalid"))
});
pub static GUILD_ID: LazyLock<Id<GuildMarker>> = LazyLock::new(|| {
  let val = env::var("BAZAAR_HAND_GUILD_ID")
    .expect("Missing BAZAAR_HAND_GUILD_ID environment variable");
  Id::new(val.parse().expect("BAZAAR_HAND_GUILD_ID is invalid"))
});
pub static DISCORD_CLIENT: LazyLock<Client> = LazyLock::new(|| {
  let token = env::var("BAZAAR_HAND_BOT_TOKEN")
    .expect("Missing BAZAAR_HAND_BOT_TOKEN environment variable");
  Client::new(token)
});
pub static DISCORD_PUBKEY: LazyLock<VerifyingKey> = LazyLock::new(|| {
  let raw = env::var("BAZAAR_HAND_PUBLIC_KEY")
    .expect("Missing BAZAAR_HAND_PUBLIC_KEY environment variable");
  let as_bytes = <[u8; PUBLIC_KEY_LENGTH]>::from_hex(raw)
    .expect("BAZAAR_HAND_PUBLIC_KEY is invalid hex");
  VerifyingKey::from_bytes(&as_bytes)
    .expect("BAZAAR_HAND_PUBLIC_KEY is not valid")
});

#[tokio::main]
async fn main() {
  tracing_subscriber::fmt()
    .with_max_level(Level::DEBUG).init();

  DISCORD_CLIENT.interaction(*APP_ID).set_global_commands(&*commands::CMDS).await.unwrap();

  let router = Router::new()
    .hoop(validate_hook)
    .post(execute_hook);

  let acceptor = TcpListener::new("0.0.0.0:1312").bind().await;

  Server::new(acceptor).serve(router).await;
}

#[handler]
async fn validate_hook(req: &mut Request, depot: &mut Depot, res: &mut Response) {
  if req.method() != Method::POST {
    debug!("Request was not POST");
    res.render(StatusError::method_not_allowed());
    return;
  }

  if req.uri().path() != "/" {
    debug!("Request was not to the root");
    res.render(StatusError::not_found());
    return;
  }

  let Some(timestamp) = req.header::<Bytes>("X-Signature-Timestamp") else {
    debug!("Timestamp missing");
    res.render(StatusError::bad_request());
    return;
  };

  let Some(sig) = req.header::<Bytes>("X-Signature-Ed25519") else {
    debug!("Signature missing");
    res.render(StatusError::bad_request());
    return;
  };

  let Ok(sig) = Vec::<u8>::from_hex(sig) else {
    debug!("Signature not valid hex");
    res.render(StatusError::bad_request());
    return;
  };

  let Ok(sig) = Signature::from_slice(sig.as_slice()) else {
    debug!("Signature is not valid");
    res.render(StatusError::bad_request());
    return;
  };

  let Ok(body) = req.take_body().collect().await.map(|x| x.to_bytes()) else {
    error!("Request body could not be collected");
    res.render(StatusError::bad_request());
    return;
  };

  let key = VerifyingKey::from_bytes(
    &<[u8; 32]>::from_hex("2f6fc6bf399e58ab166002cb4f1a5b38a51d1f788ac9a24ad916e2f317cada67").unwrap()
  ).expect("Invalid DISCORD_KEY");

  if key.verify([timestamp.as_ref(), &body].concat().as_ref(), &sig).is_err() {
    debug!("Payload failed verification");
    res.render(StatusError::unauthorized());
    return;
  }

  let Ok(interaction) = serde_json::from_slice::<Interaction>(&body) else {
    error!("Error parsing Interaction - we might need to update Twilight?");
    res.render(StatusError::internal_server_error());
    return;
  };

  debug!("Request passed validation and parsed");

  depot.insert("INTERACTION", interaction);
}

#[handler]
async fn execute_hook(depot: &mut Depot, res: &mut Response) {
  info!("Handling Discord Webhook");

  let i = depot.remove::<Interaction>("INTERACTION")
    .expect("Interaction missing");

  let result = match i.kind.clone() {
    InteractionType::Ping => Ok(InteractionResponse {
      kind: InteractionResponseType::Pong,
      data: None,
    }),
    _ => commands::handle(i.clone()).await,
  };

  let resp = result.unwrap_or_else(|e| {
    error!("Error from hook handler: {e}");
    let embed = EmbedBuilder::new()
      .title("Internal Error")
      .description(format!("{e}"))
      .validate().expect("Embed failed validation")
      .build();

    let resp = InteractionResponseDataBuilder::new()
      .flags(MessageFlags::EPHEMERAL)
      .embeds([embed])
      .build();

    InteractionResponse {
      kind: InteractionResponseType::ChannelMessageWithSource,
      data: Some(resp),
    }
  });

  res.render(Json(resp));
}
