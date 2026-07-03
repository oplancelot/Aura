fn main() {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let config = match profile.as_str() {
        "release" => "Release",
        _ => "Debug",
    };

    // Build SenseVoice.cpp + ggml via cmake.
    // We pass /utf-8 (for Chinese comments in source) and /EHsc (C++ exceptions),
    // overriding the GCC-style flags the upstream CMakeLists.txt appends on MSVC.
    let dst = cmake::Config::new("3rdparty/SenseVoice.cpp")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("SENSE_VOICE_BUILD_EXAMPLES", "OFF")
        .define("CMAKE_CXX_FLAGS_DEBUG", "/nologo /MD /utf-8 /EHsc")
        .define("CMAKE_CXX_FLAGS_RELEASE", "/nologo /MD /utf-8 /EHsc /O2")
        .very_verbose(true)
        .build();

    // SenseVoice.cpp doesn't install sense-voice-core.lib, so we need to find it
    // in the build tree under the configuration-specific subdirectory.
    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!(
        "cargo:rustc-link-search=native={}/build/lib/{}",
        dst.display(),
        config
    );
    println!("cargo:rustc-link-lib=static=sense-voice-core");
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-base");
    println!("cargo:rustc-link-lib=static=ggml-cpu");

    // Build C wrapper via cc crate
    cc::Build::new()
        .cpp(true)
        .file("src/ai/sense_voice_capi.cc")
        .include("3rdparty/SenseVoice.cpp/sense-voice/csrc")
        .include("3rdparty/SenseVoice.cpp/sense-voice/csrc/third-party/ggml/include")
        .compile("aura_sense_voice_capi");

    println!("cargo:rerun-if-changed=src/ai/sense_voice_capi.cc");
    println!("cargo:rerun-if-changed=src/ai/sense_voice_capi.h");
    println!("cargo:rerun-if-changed=3rdparty/SenseVoice.cpp");
}
