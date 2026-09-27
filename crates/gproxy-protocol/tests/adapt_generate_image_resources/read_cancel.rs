use super::*;
#[test]
fn canceled_partial_resource_read_resumes_body_and_derives_missing_mime() {
    let store = Arc::new(Store::default());
    let state = state(&store, Dialect::Gemini);
    let resource_host = Resources::default();
    resource_host.omit_read_mime.store(true, Ordering::SeqCst);
    let feed = Feed::default();
    let bytes = png();
    let middle = bytes.len() / 2;
    feed.push(bytes.slice(..middle));
    *resource_host.read_feed.lock().unwrap() = Some(feed.clone());
    let mut resources = resources(&resource_host);
    resources.max_references = 1;
    let mut native = gresponse();
    native.candidates.as_mut().unwrap()[0]
        .content
        .as_mut()
        .unwrap()
        .parts
        .as_mut()
        .unwrap()[0]
        .file_data
        .as_mut()
        .unwrap()
        .mime_type = None;
    let host = host(store.clone(), &native);
    let mut call = ResponsesViaGemini::prepare(
        rrequest(),
        "selected",
        Endpoint::new("/generateContent").unwrap(),
        ids(Dialect::OpenAi, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let mut progress = ImageResourceProgress::default();
    {
        let mut pending = Box::pin(call.invoke_with_image_resources(
            &host,
            &(),
            limits(),
            &state,
            &resources,
            &mut progress,
            |_| Ok(facts()),
        ));
        assert!(
            pending
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
    assert_eq!(progress.reads.bytes_read(), middle as u64);
    assert_eq!(progress.reads.attempted_reads(), 1);
    feed.push(bytes.slice(middle..));
    feed.close();
    let GenerationOutcome::Success { response, .. } = ready(call.recover_with_image_resources(
        limits(),
        &state,
        &resources,
        &mut progress,
        |_| Ok(facts()),
    ))
    .unwrap() else {
        panic!()
    };
    let Some(r::ResponseOutputItem::ImageGenerationCall(image)) = response
        .body
        .output
        .iter()
        .find(|item| matches!(item, r::ResponseOutputItem::ImageGenerationCall(_)))
    else {
        panic!()
    };
    assert_eq!(image.result.as_deref(), Some(PNG));
    assert_eq!(progress.reads.bytes_read(), bytes.len() as u64);
    assert_eq!(resource_host.reads.load(Ordering::SeqCst), 1);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn invalid_materialized_mime_releases_live_native_stream_immediately() {
    struct Tracked {
        feed: Feed,
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }
    impl futures_core::Stream for Tracked {
        type Item = Result<bytes::Bytes, gproxy_protocol::connection::TransportError>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            futures_core::Stream::poll_next(std::pin::Pin::new(&mut self.get_mut().feed), cx)
        }
    }
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }
    let store = Arc::new(Store::default());
    let state = state(&store, Dialect::Gemini);
    let resource_host = Resources::default();
    let resources = resources(&resource_host);
    let request = rrequest();
    let f = facts();
    let mut call = ready(ResponsesViaGemini::prepare_stream(
        request.clone(),
        StreamTarget {
            endpoint: Endpoint::new("/streamGenerateContent").unwrap(),
            identities: ids(Dialect::OpenAi, Dialect::Gemini),
        },
        ResponsesViaGeminiStreamFacts {
            request: Default::default(),
            response: pair::stream::GeminiToResponsesContext {
                response: pair::GeminiResponseContext {
                    request,
                    effective_parallel_tool_calls: f.parallel_tool_calls,
                    effective_tool_choice: f.tool_choice,
                    effective_prompt_cache_options: f.prompt_cache_options,
                    usage: f.usage,
                    created_at: f.created_at,
                },
                actual_model: Some("selected".into()),
                final_thinking_tokens: None,
            },
        },
        settings(),
        &state,
    ))
    .unwrap();
    let feed = Feed::default();
    let mut native = gresponse();
    native.candidates.as_mut().unwrap()[0]
        .content
        .as_mut()
        .unwrap()
        .parts
        .as_mut()
        .unwrap()[0]
        .file_data
        .as_mut()
        .unwrap()
        .mime_type = Some("image/jpeg".into());
    feed.push(format!(
        "data: {}\n\n",
        serde_json::to_string(&native).unwrap()
    ));
    let host = Host::stream(store.clone(), feed.clone());
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    host.response.lock().unwrap().as_mut().unwrap().body = HttpBody::Stream(Box::pin(Tracked {
        feed,
        dropped: dropped.clone(),
    }));
    ready(call.start(&host, &(), &state)).unwrap();
    let mut progress = ImageStreamProgress::default();
    assert!(ready(call.next_with_image_resources(&state, &resources, &mut progress)).is_err());
    assert!(dropped.load(Ordering::SeqCst));
    assert!(call.client_result().is_none());
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn signed_file_history_replays_from_the_client_bytes_after_the_resource_expires() {
    let store = Arc::new(Store::default());
    let mut access = state(&store, Dialect::Gemini);
    let resource_host = Resources::default();
    *resource_host.read_expiry.lock().unwrap() = Some(UNIX_EPOCH + Duration::from_secs(5));
    let resources = resources(&resource_host);
    let native = gresponse();
    let host = host(store.clone(), &native);
    let mut call = ResponsesViaGemini::prepare(
        rrequest(),
        "selected",
        Endpoint::new("/generateContent").unwrap(),
        ids(Dialect::OpenAi, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let mut progress = ImageResourceProgress::default();
    let GenerationOutcome::Success { response, .. } = ready(call.invoke_with_image_resources(
        &host,
        &(),
        limits(),
        &access,
        &resources,
        &mut progress,
        |_| Ok(facts()),
    ))
    .unwrap() else {
        panic!()
    };
    // The replay needs neither the resource nor any state: the client sends
    // the bytes back, and its reasoning item carries the signature.
    let items: Vec<r::InputItem> = response
        .body
        .output
        .iter()
        .map(|item| serde_json::from_value(serde_json::to_value(item).unwrap()).unwrap())
        .collect();
    let mut input = rrequest();
    input.input = Some(r::Input::Items(items));
    for now in [4, 5, 10] {
        access.now = UNIX_EPOCH + Duration::from_secs(now);
        let prepared = ready(ResponsesViaGemini::prepare_with_state(
            input.clone(),
            Endpoint::new("/generateContent").unwrap(),
            ids(Dialect::OpenAi, Dialect::Gemini),
            &access,
            Default::default(),
        ))
        .unwrap();
        let part = &prepared.target_request().contents[0]
            .parts
            .as_ref()
            .unwrap()[0];
        assert_eq!(part.inline_data.as_ref().unwrap().data, PNG);
        assert_eq!(
            part.thought_signature.as_deref(),
            Some("original-signed-file")
        );
    }
    assert_eq!(resource_host.reads.load(Ordering::SeqCst), 1);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
