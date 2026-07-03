using System;
using System.Runtime.InteropServices;

namespace Aura.Interop;

/// <summary>
/// Managed wrapper around the aura_core.dll Rust native library.
/// Provides lifecycle management and callback registration for the
/// audio capture → VAD → AI translation pipeline.
/// </summary>
public static class AuraCoreBinding
{
    private const string DllName = "aura_core";

    // ── Callback delegate (must match Rust's TranslationCallback signature) ──

    /// <summary>
    /// Delegate matching the Rust FFI callback:
    ///   fn(text: *const c_char, is_provisional: c_int, latency_ms: c_int)
    /// </summary>
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    public delegate void TranslationCallbackDelegate(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string text,
        int isProvisional,
        int latencyMs);

    // Keep a reference to prevent GC collection of the delegate
    private static TranslationCallbackDelegate? _pinnedCallback;

    // ── Imported functions ──────────────────────────────────────────

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int aura_core_init();

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void aura_core_register_callback(TranslationCallbackDelegate cb);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int aura_core_start(uint targetPid);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int aura_core_stop();

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void aura_core_destroy();

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int aura_core_set_engine(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string engineName);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void aura_core_set_api_key(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string apiKey);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void aura_core_set_target_lang(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string lang);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void aura_core_set_model_path(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string path);

    // ── Public API ──────────────────────────────────────────────────

    /// <summary>Initialise the core pipeline. Call once at startup.</summary>
    public static int Init() => aura_core_init();

    /// <summary>Register a managed callback for translation results.</summary>
    public static void RegisterCallback(TranslationCallbackDelegate callback)
    {
        _pinnedCallback = callback;  // prevent GC
        aura_core_register_callback(_pinnedCallback);
    }

    /// <summary>Start capturing and translating audio from the target process.</summary>
    public static int Start(uint targetPid) => aura_core_start(targetPid);

    /// <summary>Stop the capture and translation pipeline.</summary>
    public static int Stop() => aura_core_stop();

    /// <summary>Destroy the core pipeline and free all resources.</summary>
    public static void Destroy() => aura_core_destroy();

    /// <summary>Switch the active AI engine ("gemini" or "sensevoice").</summary>
    public static int SetEngine(string engineName) => aura_core_set_engine(engineName);

    /// <summary>Set the API key for cloud AI engines.</summary>
    public static void SetApiKey(string apiKey) => aura_core_set_api_key(apiKey);

    /// <summary>Set the target translation language (ISO 639-1).</summary>
    public static void SetTargetLang(string lang) => aura_core_set_target_lang(lang);

    /// <summary>Set the path to the Silero VAD ONNX model file.</summary>
    public static void SetModelPath(string path) => aura_core_set_model_path(path);
}
