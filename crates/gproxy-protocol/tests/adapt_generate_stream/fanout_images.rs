use super::*;
use gproxy_protocol::{
    adapt::generate::image_resources::ImageStreamProgress,
    transform::{generate::stream::responses::synthesize_responses_stream, identity::IdentityFlow},
    wire::{gemini as g, openai::responses as r},
};
fn image_resources(
    host: &image_host::Resources,
    max_references: usize,
) -> GenerationResources<'_, image_host::Resources> {
    GenerationResources {
        access: host,
        scope: &1,
        limits: settings().codec,
        max_references,
        now: UNIX_EPOCH,
    }
}
fn image_feed() -> Feed {
    let mut body = all_pairs::native_response(Dialect::OpenAi);
    body["output"] = json!([{"type":"image_generation_call","id":"ig-native","result":image_host::PNG,"status":"completed"}]);
    let body: r::GenerateContentResponseBody = serde_json::from_value(body).unwrap();
    let events = synthesize_responses_stream(
        body,
        &mut IdentityFlow::new(IdNamespace([93; 16])),
        Default::default(),
    )
    .unwrap()
    .value;
    let feed = Feed::default();
    for event in events {
        feed.push(format!(
            "event: {}\ndata: {}\n\n",
            event.event_name().unwrap(),
            serde_json::to_string(&event).unwrap()
        ));
    }
    feed
}
fn prepare_images(
    store: &Store,
    resources: &GenerationResources<'_, image_host::Resources>,
) -> FanoutStream<gr::ResponsesToGeminiStream> {
    let mut input = request(Dialect::Gemini);
    input["generationConfig"]["responseModalities"] = json!(["TEXT", "IMAGE"]);
    input["generationConfig"]["responseFormat"] = json!({"image":{"delivery":"URI"}});
    ready(GeminiViaResponsesFanout::prepare_stream_with_capabilities(
        serde_json::from_value(input).unwrap(),
        target(Dialect::Gemini, Dialect::OpenAi),
        {
            let mut contexts = vec![Default::default(), Default::default()].into_iter();
            move |_| contexts.next().unwrap()
        },
        settings(),
        &access(store, Dialect::OpenAi),
        resources,
    ))
    .unwrap()
}
fn first_uri(event: &g::GenerateContentResponseBody) -> Option<&str> {
    event
        .candidates
        .iter()
        .flatten()
        .filter_map(|c| c.content.as_ref())
        .flat_map(|c| c.parts.iter().flatten())
        .find_map(|p| p.file_data.as_ref().map(|f| f.file_uri.as_str()))
}
#[test]
fn image_fanout_publishes_before_first_eof_and_recovers_without_resending() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::OpenAi);
    let resource_host = image_host::Resources::default();
    let resources = image_resources(&resource_host, 2);
    let mut call = prepare_images(&store, &resources);
    let first = image_feed();
    let second = image_feed();
    second.close();
    let host = MultiHost::new(store.clone(), vec![first.clone(), second]);
    assert!(ready(call.next(&host, &(), &state)).is_err());
    assert!(
        host.host.sent.lock().unwrap().is_empty(),
        "ordinary next must reject before POST"
    );
    let mut progress: Vec<ImageStreamProgress<String>> =
        (0..2).map(|_| Default::default()).collect();
    #[cfg(not(target_arch = "wasm32"))]
    {
        fn assert_send<T: Send>(_: &T) {}
        let normal = call.next(&host, &(), &state);
        assert_send(&normal);
        drop(normal);
        let mapped = call.next_with_image_resources(&host, &(), &state, &resources, &mut progress);
        assert_send(&mapped);
        drop(mapped);
    }
    resource_host.hang_publish.store(true, Ordering::SeqCst);
    {
        let mut next =
            Box::pin(call.next_with_image_resources(&host, &(), &state, &resources, &mut progress));
        for _ in 0..10000 {
            assert!(
                next.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            if resource_host.hung.load(Ordering::SeqCst) {
                break;
            }
        }
    }
    assert!(resource_host.hung.load(Ordering::SeqCst));
    assert_eq!(resource_host.publishes.load(Ordering::SeqCst), 1);
    assert_eq!(host.host.sent.lock().unwrap().len(), 1);
    let mut fresh: Vec<ImageStreamProgress<String>> = (0..2).map(|_| Default::default()).collect();
    assert!(
        ready(call.next_with_image_resources(&host, &(), &state, &resources, &mut fresh)).is_err()
    );
    assert_eq!(resource_host.publishes.load(Ordering::SeqCst), 1);
    let mut early = false;
    for _ in 0..8 {
        let chunk =
            ready(call.next_with_image_resources(&host, &(), &state, &resources, &mut progress))
                .unwrap()
                .unwrap();
        if let Some(event) = chunk.event
            && let Some(uri) = first_uri(&event)
        {
            assert!(uri.starts_with("https://published.example/"));
            assert!(
                !serde_json::to_string(&event)
                    .unwrap()
                    .contains("inlineData")
            );
            early = true;
            break;
        }
    }
    assert!(early);
    assert_eq!(host.host.sent.lock().unwrap().len(), 1);
    assert!(call.client_result().is_none());
    first.close();
    let full =
        ready(call.collect_with_image_resources(&host, &(), &state, &resources, &mut progress))
            .unwrap()
            .value;
    let candidates = full.candidates.unwrap();
    assert_eq!(candidates.len(), 2);
    let urls: Vec<_> = candidates
        .iter()
        .map(|c| {
            c.content.as_ref().unwrap().parts.as_ref().unwrap()[0]
                .file_data
                .as_ref()
                .unwrap()
                .file_uri
                .as_str()
        })
        .collect();
    assert_ne!(urls[0], urls[1]);
    assert_eq!(resource_host.publishes.load(Ordering::SeqCst), 2);
    assert_eq!(host.host.sent.lock().unwrap().len(), 2);
    assert!(resource_host.statuses.load(Ordering::SeqCst) >= 1);
}
#[test]
fn image_fanout_reference_limit_is_shared_across_children() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::OpenAi);
    let resource_host = image_host::Resources::default();
    let resources = image_resources(&resource_host, 1);
    let mut call = prepare_images(&store, &resources);
    let feeds = (0..2)
        .map(|_| {
            let f = image_feed();
            f.close();
            f
        })
        .collect();
    let host = MultiHost::new(store.clone(), feeds);
    let mut progress: Vec<ImageStreamProgress<String>> =
        (0..2).map(|_| Default::default()).collect();
    assert!(
        ready(call.collect_with_image_resources(&host, &(), &state, &resources, &mut progress))
            .is_err()
    );
    assert_eq!(resource_host.publishes.load(Ordering::SeqCst), 1);
    assert!(call.client_result().is_none());
    assert!(call.children()[0].native_result().is_some());
}

#[test]
fn replacing_completed_child_resource_progress_cannot_reset_group_budget() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::OpenAi);
    let resource_host = image_host::Resources::default();
    let resources = image_resources(&resource_host, 1);
    let mut call = prepare_images(&store, &resources);
    let first = image_feed();
    first.close();
    let second = Feed::default();
    let host = MultiHost::new(store.clone(), vec![first, second]);
    let mut progress: Vec<ImageStreamProgress<String>> =
        (0..2).map(|_| Default::default()).collect();
    for _ in 0..10000 {
        let mut next =
            Box::pin(call.next_with_image_resources(&host, &(), &state, &resources, &mut progress));
        match next.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            std::task::Poll::Ready(Ok(Some(_))) => {}
            std::task::Poll::Pending if host.host.sent.lock().unwrap().len() == 2 => break,
            std::task::Poll::Pending => {}
            _ => panic!("unexpected outcome before second native response"),
        }
    }
    assert_eq!(host.host.sent.lock().unwrap().len(), 2);
    assert!(call.children()[0].client_result().is_some());
    assert_eq!(resource_host.publishes.load(Ordering::SeqCst), 1);
    let _retained_receipts = std::mem::take(&mut progress[0]);
    let mut next =
        Box::pin(call.next_with_image_resources(&host, &(), &state, &resources, &mut progress));
    assert!(matches!(
        next.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        std::task::Poll::Ready(Err(_))
    ));
    assert_eq!(resource_host.publishes.load(Ordering::SeqCst), 1);
}
