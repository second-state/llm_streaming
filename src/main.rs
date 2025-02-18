mod app;
mod config;
mod llm;
mod stream_platform;

#[tokio::main]
async fn main() {
    env_logger::init();
    let config_path = std::env::args().nth(1).unwrap_or("config.toml".to_string());
    let config_str = std::fs::read_to_string(config_path).unwrap();
    let config: config::Config = toml::from_str(&config_str).unwrap();
    log::info!("{:#?}", config);

    let listener = tokio::net::TcpListener::bind(&config.listen).await.unwrap();

    let (stream_tx, stream_rx) = tokio::sync::mpsc::unbounded_channel();
    match config.platform {
        config::StreamPlatFormConfig::Bilibili(bilibili) => {
            let max_conment = bilibili.max_comment;
            let client = stream_platform::bilibili::BiliLiveClient::from_config(bilibili)
                .expect("bilibili client");
            tokio::spawn(stream_platform::llm_loop(max_conment, stream_rx, client));
        }
        config::StreamPlatFormConfig::Restream(restream) => {
            let max_conment = restream.max_comment;
            let client = stream_platform::restream::RestreamChat::from_config(restream)
                .await
                .expect("restream chat");
            tokio::spawn(stream_platform::llm_loop(max_conment, stream_rx, client));
        }
    }

    log::info!("Start on {}", &config.listen);
    let app = app::router(config.llm, config.tts, config.downstream, stream_tx);
    axum::serve(listener, app).await.unwrap();
}
