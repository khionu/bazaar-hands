use snafu::Whatever;
use tokio::time::Instant;
use twilight_model::http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType};
use twilight_util::builder::InteractionResponseDataBuilder;

pub async fn cmd(i_token: String) -> Result<InteractionResponse, Whatever> {
  tokio::spawn(async move {
    const ITERATIONS: usize = 8;
    let mut rt_set = [0u32; ITERATIONS];
    let i_client = crate::DISCORD_CLIENT.interaction(*crate::APP_ID);
    for i in 0..ITERATIONS {
      let content = format!("Pinging ...");
      let start = Instant::now();
      i_client.update_response(i_token.as_str())
        .content(Some(content.as_str())).await
        .expect("failed to create followup message");
      let rt = start.elapsed().as_millis();
      rt_set[i] = rt as u32;
    }
    let avg = dbg!(rt_set).iter().sum::<u32>() / ITERATIONS as u32;
    i_client.update_response(i_token.as_str())
      .content(Some(format!("Average RT ping: {avg}ms").as_str())).await
      .expect("failed to create followup message");
  });
  Ok(InteractionResponse {
    kind: InteractionResponseType::ChannelMessageWithSource,
    data: Some(InteractionResponseDataBuilder::new()
      .content("<:loading:1353501077659586590>")
      .build()),
  })
}