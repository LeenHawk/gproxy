use super::*;
#[test]
fn prepared_generation_sends_only_native_media_and_attaches_fallback_warnings_to_results_and_streams()
 {
    futures_executor::block_on(async {
        let req = request(Dialect::Openai, image(reference(), Some("image/png")));
        for streaming in [false, true] {
            let client = Client::new(Script {
                sent: Mutex::new(vec![]),
                stream: streaming,
            });
            let prepared = req
                .resolve_assets(DeliveryMode::PreferUrl, |_| async {
                    Ok(bytes("image/png"))
                })
                .await
                .unwrap();
            let warnings = prepared.warnings().to_vec();
            assert_eq!(warnings.len(), 1);
            if streaming {
                let mut stream = client.stream_prepared(prepared).await.unwrap();
                assert!(
                    matches!(stream.next().await,Some(Event::Start{warnings:w}) if w==warnings)
                );
                let events = stream.collect::<Vec<_>>().await;
                assert!(events.iter().any(|e| matches!(e, Event::Finish { .. })));
            } else {
                let result = client.generate_prepared(prepared).await.unwrap();
                assert_eq!(result.warnings, warnings);
            }
            let sent = client.backend.sent.lock().unwrap();
            assert_eq!(sent.len(), 1);
            assert_eq!(
                sent[0]["input"][0]["content"][0]["image_url"],
                "data:image/png;base64,AQID"
            );
            assert!(!sent[0].to_string().contains("blob:sha256"));
        }
    });
}
