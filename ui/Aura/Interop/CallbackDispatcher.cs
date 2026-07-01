using System;
using System.Windows.Threading;

namespace Aura.Interop;

/// <summary>
/// Dispatches translation callbacks from the Rust background thread
/// to the WPF UI thread (Dispatcher) for safe rendering updates.
/// </summary>
public class CallbackDispatcher
{
    private readonly Dispatcher _dispatcher;
    private readonly Action<string, bool, int> _handler;

    /// <summary>
    /// Create a new dispatcher.
    /// </summary>
    /// <param name="dispatcher">The WPF UI thread dispatcher.</param>
    /// <param name="handler">
    /// Action receiving (translatedText, isProvisional, latencyMs).
    /// </param>
    public CallbackDispatcher(Dispatcher dispatcher, Action<string, bool, int> handler)
    {
        _dispatcher = dispatcher;
        _handler = handler;
    }

    /// <summary>
    /// Callback method compatible with <see cref="AuraCoreBinding.TranslationCallbackDelegate"/>.
    /// Marshals the call from the Rust thread to the UI thread.
    /// </summary>
    public void OnTranslation(string text, int isProvisional, int latencyMs)
    {
        _dispatcher.BeginInvoke(DispatcherPriority.Normal, () =>
        {
            _handler(text, isProvisional != 0, latencyMs);
        });
    }
}
