use futures_util::stream::StreamExt;
use tokio::net::TcpStream;
use tokio_websockets::MaybeTlsStream;

pub struct RestreamChat {
    client: tokio_websockets::WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl RestreamChat {
    pub async fn new(uri: &str) -> anyhow::Result<Self> {
        let (client, resp) = tokio_websockets::ClientBuilder::new()
            .uri(uri)?
            .connect()
            .await?;
        if resp.status() == 101 {
            Ok(Self { client })
        } else {
            Err(anyhow::anyhow!("Failed to connect to {} {:?}", uri, resp))
        }
    }
}

#[allow(unused)]
#[derive(Debug, serde::Deserialize)]
struct RestreamEvent {
    #[serde(default)]
    action: String,
    payload: RestreamPayload,
    #[serde(default)]
    timestamp: u64,
}

#[allow(unused)]
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestreamPayload {
    #[serde(default)]
    connection_identifier: String,
    #[serde(default)]
    event_identifier: String,
    event_payload: RestreamEventPayload,
    #[serde(default)]
    event_source_id: u32,
    #[serde(default)]
    event_type_id: u32,
    #[serde(default)]
    user_id: u64,
}

#[derive(Debug, serde::Deserialize)]
struct RestreamEventPayload {
    author: Author,
    #[serde(default)]
    text: String,
}

#[allow(unused)]
#[derive(Debug, serde::Deserialize)]
struct Author {
    #[serde(default)]
    id: String,
    #[serde(default)]
    #[serde(rename = "displayName")]
    name: String,
}

impl super::StreamPlatform for RestreamChat {
    async fn next_event(&mut self) -> anyhow::Result<super::SteamEvent> {
        loop {
            let msg = self
                .client
                .next()
                .await
                .ok_or(anyhow::anyhow!("restream ws closed!"))??;
            if let Some(text) = msg.as_text() {
                if let Ok(event) = serde_json::from_str::<RestreamEvent>(&text) {
                    return Ok(super::SteamEvent::Comment {
                        user: event.payload.event_payload.author.name,
                        content: event.payload.event_payload.text,
                    });
                } else {
                    log::debug!("Failed to parse restream event: {}", text);
                }
            }
        }
    }
}

impl RestreamChat {
    pub async fn from_config(config: crate::config::RestreamConfig) -> anyhow::Result<Self> {
        Ok(Self::new(&config.url).await?)
    }
}

#[test]
fn test() {
    let json = r#"{"action":"event","payload":{"connectionIdentifier":"9209300-youtube-fyoek8R2vEQ","eventIdentifier":"cbaa276f8c5a583412fda48e7eff2bb2","eventPayload":{"author":{"avatar":"https://yt3.ggpht.com/ytc/AIdro_ldJnZ7mv3rFn-tjv592UgHNAiyYH_VZ-mvhJU2F94=s120-c-k-c0x00ffffff-no-rj","displayName":"Vivian Hu","id":"UC7xT5iEi6tzxdY6w1L2PI3A","isChatModerator":false,"isChatOwner":false,"isChatSponsor":false,"isVerified":false},"bot":false,"liveChatMessageId":"LCC.EhwKGkNQMldwS3lNMVlzREZUOF9yUVlkX0lrNkl3","text":"test"},"eventSourceId":13,"eventTypeId":5,"userId":9209300},"timestamp":1740152273}"#;
    let event = serde_json::from_str::<RestreamEvent>(json);
    println!("{:?}", event);
}
