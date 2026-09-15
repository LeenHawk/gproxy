#![cfg(all(feature = "fs", not(target_arch = "wasm32")))]

use gproxy_file::{Buffer, ErrorKind, filesystem};

#[tokio::test]
async fn local_files_roundtrip_overwrite_list_and_delete() {
    let directory = tempfile::tempdir().unwrap();
    let files = filesystem(directory.path().to_str().unwrap()).unwrap();
    let data: Vec<u8> = (0..=255).collect();
    files.write("nested/中文.bin", data.clone()).await.unwrap();
    assert_eq!(files.read("nested/中文.bin").await.unwrap().to_vec(), data);
    assert_eq!(
        files
            .stat("nested/中文.bin")
            .await
            .unwrap()
            .content_length(),
        256
    );
    let entries = files.list("nested/").await.unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry.path() == "nested/中文.bin")
    );
    files.write("nested/中文.bin", Buffer::new()).await.unwrap();
    assert!(files.read("nested/中文.bin").await.unwrap().is_empty());
    files.delete("nested/中文.bin").await.unwrap();
    assert_eq!(
        files.read("nested/中文.bin").await.unwrap_err().kind(),
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn streaming_writer_and_range_reader_use_the_same_store() {
    let directory = tempfile::tempdir().unwrap();
    let files = filesystem(directory.path().to_str().unwrap()).unwrap();
    let mut writer = files.writer("stream.bin").await.unwrap();
    writer.write("abc").await.unwrap();
    writer.write("def").await.unwrap();
    writer.close().await.unwrap();
    assert_eq!(
        files
            .read_with("stream.bin")
            .range(1..5)
            .await
            .unwrap()
            .to_vec(),
        b"bcde"
    );
}
