fn main() {
    // whisper.cpp's Metal objects use Clang's `__isPlatformVersionAtLeast`
    // availability helper. rustc links native static archives with
    // `-nodefaultlibs`, so explicitly include Apple Clang's universal runtime
    // archive for macOS app binaries.
    if std::env::var("TARGET").is_ok_and(|target| target.ends_with("apple-darwin")) {
        let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let target = std::env::var("TARGET").unwrap();
        let swift_target = if target.starts_with("aarch64-") {
            "arm64-apple-macosx14.0"
        } else {
            "x86_64-apple-macosx14.0"
        };
        let swift_source = manifest.join("macos/SystemAudioCapture.swift");
        let swift_object = out_dir.join("SystemAudioCapture.o");
        let status = std::process::Command::new("xcrun")
            .args(["swiftc", "-swift-version", "5", "-O", "-parse-as-library", "-target", swift_target, "-c"])
            .arg(&swift_source)
            .arg("-o")
            .arg(&swift_object)
            .status()
            .expect("xcrun swiftc is required to build macOS system-audio capture");
        assert!(status.success(), "failed to compile ScreenCaptureKit audio bridge");
        println!("cargo:rerun-if-changed={}", swift_source.display());
        println!("cargo:rustc-link-arg-bin=openflow-tauri={}", swift_object.display());
        for framework in ["ScreenCaptureKit", "AVFoundation", "CoreMedia", "CoreAudio"] {
            println!("cargo:rustc-link-arg-bin=openflow-tauri=-framework");
            println!("cargo:rustc-link-arg-bin=openflow-tauri={framework}");
        }

        if let Ok(clang) = std::process::Command::new("xcrun")
            .args(["--find", "clang"])
            .output()
        {
            if clang.status.success() {
                let clang = String::from_utf8_lossy(&clang.stdout).trim().to_string();
                if let Ok(resource) = std::process::Command::new(clang)
                    .arg("-print-resource-dir")
                    .output()
                {
                    if resource.status.success() {
                        let runtime = format!(
                            "{}/lib/darwin/libclang_rt.osx.a",
                            String::from_utf8_lossy(&resource.stdout).trim()
                        );
                        println!("cargo:rustc-link-arg-bins={runtime}");
                    }
                }
            }
        }
    }

    // The publik app token is read at compile time (`option_env!("PUBLIK_APP_TOKEN")`
    // in src/publik.rs). Cargo's fingerprint does not include env vars unless told,
    // so without this line a cached object keeps whatever value it had — the
    // classic "the release shipped with no token" failure.
    println!("cargo:rerun-if-env-changed=PUBLIK_APP_TOKEN");
    tauri_build::build();
}
