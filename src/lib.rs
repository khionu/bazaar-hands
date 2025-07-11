use std::cell::OnceCell;
// Standard lib
use std::env;
use std::io::BufReader;
use std::sync::{LazyLock, OnceLock, OnceState};
// Basic utilities
use bytes::Bytes;
use ed25519_dalek::{Signature, Verifier, VerifyingKey, PUBLIC_KEY_LENGTH};
use hex::FromHex;
use snafu::{prelude::*, Whatever};
use tracing::{debug, error, info, Level};
// HTTP
use http_body_util::BodyExt;
use spin_sdk::http::{IntoResponse, Json, Method, Request, Response, ResponseBuilder};
use spin_sdk::http::conversions::TryIntoBody;
use spin_sdk::http::responses::{internal_server_error, method_not_allowed, not_found};
use spin_sdk::http_component;
use spin_sdk::wit_bindgen::rt::invalid_enum_discriminant;
use tracing::field::debug;
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

/// A simple Spin HTTP component.
#[http_component]
async fn discord(req: Request) -> Response {
  if *req.method() != Method::Post {
    debug!("Request was not POST");
    return method_not_allowed();
  }

  if req.uri() != "https://boba.iamtrue.net/api/discord/" {
    debug!("Bad URI: got {}", req.uri());
    return not_found();
  }

  let Some(timestamp) = req
    .header("X-Signature-Timestamp")
    .map(|x| Bytes::copy_from_slice(x.as_bytes())) else {
    debug!("Timestamp missing");
    return invalid_request();
  };

  let Some(sig) = req
    .header("X-Signature-Ed25519")
    .map(|x| Bytes::copy_from_slice(x.as_bytes())) else {
    debug!("Signature missing");
    return invalid_request();
  };

  let Ok(sig) = Vec::<u8>::from_hex(sig) else {
    debug!("Signature not valid hex");
    return invalid_request();
  };

  let Ok(sig) = Signature::from_slice(sig.as_slice()) else {
    debug!("Signature is not valid");
    return invalid_request();
  };

  if DISCORD_PUBKEY.verify(&[timestamp.as_ref(), req.body()].concat(), &sig).is_err() {
    debug!("Payload failed verification");
    return invalid_request();
  }

  let Ok(i) = serde_json::from_slice::<Interaction>(req.body()) else {
    error!("Error parsing Interaction - we might need to update Twilight?");
    return internal_server_error();
  };

  debug!("Request passed validation and parsed");
  info!("Handling Discord Webhook");

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
  
  let resp = Json(resp).try_into_body()
    .expect("Twilight's JSON serialization failed");

  Response::builder().status(200).body(resp).build()
}

fn invalid_request() -> Response {
  Response::builder().status(401).body("invalid request signature").build()
}

#[unsafe(export_name = "wizer.initialize")]
pub extern "C" fn init() {
  futures::executor::block_on(async {
    DISCORD_CLIENT.interaction(*APP_ID).set_global_commands(&*commands::CMDS).await
      .expect("Failed to set global commands");
  });
}
