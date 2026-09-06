use serde_json::json;
use stimma_drawthings::{catalog, generation, install, media, proto::MetadataOverride, tensor};

#[test]
fn advanced_configuration_survives_wire_roundtrip() {
    let models = MetadataOverride {
        models: serde_json::to_vec(
            &json!([{"name":"SDXL","version":"sdxl_base_v0.9","file":"sd_xl_base_test.ckpt","stp_installed":true}]),
        )
        .unwrap(),
        ..Default::default()
    };
    let (profile, _, p) = catalog::prepare("sdxl", &json!({"prompt":"test", "native_configuration":{"tiled_decoding":true,"decoding_tile_width":8,"separate_clip_l":true,"clip_l_text":"alternate","tea_cache":true}}), &models).unwrap();
    let (wire, _) = generation::configuration(&profile, &p, false).unwrap();
    let c = stimma_drawthings::generated::stimma_drawthings::_generated::config::root_as_generation_configuration(&wire).unwrap();
    assert!(c.tiled_decoding());
    assert_eq!(c.decoding_tile_width(), 8);
    assert_eq!(c.clip_l_text(), Some("alternate"));
    assert!(c.tea_cache());
    assert!(catalog::prepare(
        "sdxl",
        &json!({"prompt":"test","native_configuration":{"typo":true}}),
        &models
    )
    .is_err());
    assert!(catalog::prepare(
        "sdxl",
        &json!({"prompt":"test","native_configuration":{"sampler":"typo"}}),
        &models
    )
    .is_err());
}
#[test]
fn image_and_mask_encoding_preserves_semantics() {
    let image = image::RgbImage::from_pixel(2, 3, image::Rgb([0, 255, 0]));
    let png = tensor::pngs(&tensor::encode_image(&image))
        .unwrap()
        .remove(0);
    assert_eq!(image::load_from_memory(&png).unwrap().to_rgb8(), image);
    let mask = image::GrayImage::from_raw(3, 1, vec![0, 127, 255]).unwrap();
    assert_eq!(&tensor::encode_mask(&mask, false)[68..], &[0, 0, 2]);
    assert_eq!(&tensor::encode_mask(&mask, true)[68..], &[0, 0, 255]);
}
#[tokio::test]
async fn assets_cannot_escape_root() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["../secret", "..", "a/b", "a\\b", "a:b", ""] {
        assert!(install::safe_name(name).is_err());
    }
    tokio::fs::write(dir.path().join("ok.png"), b"fixture")
        .await
        .unwrap();
    assert!(media::asset(dir.path(), "ok.png").await.is_ok());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/hosts", dir.path().join("escape")).unwrap();
        assert!(media::asset(dir.path(), "escape").await.is_err());
    }
}
#[tokio::test]
async fn downloads_are_verified_atomic_and_work_offline_after_install() {
    use sha2::{Digest, Sha256};
    let router = axum::Router::new().route(
        "/component",
        axum::routing::get(|| async { "verified component" }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("component");
    let url = format!("http://{addr}/component");
    assert!(install::fetch(&url, &"0".repeat(64), &dest, None, false)
        .await
        .is_err());
    assert!(!dest.exists());
    let sha = hex::encode(Sha256::digest(b"verified component"));
    install::fetch(&url, &sha, &dest, None, false)
        .await
        .unwrap();
    server.abort();
    install::fetch(&url, &sha, &dest, None, true).await.unwrap();
    tokio::fs::write(&dest, b"corrupt").await.unwrap();
    assert!(install::fetch(&url, &sha, &dest, None, true).await.is_err());
}
