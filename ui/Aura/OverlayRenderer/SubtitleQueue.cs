using System;
using System.Collections.Concurrent;
using System.Collections.Generic;

namespace Aura.OverlayRenderer;

/// <summary>
/// Thread-safe queue managing visible subtitle entries on screen.
///
/// Handles:
/// - Provisional text replacement (new provisional overwrites previous)
/// - Final text commit (replaces any provisional, stays visible for N seconds)
/// - Auto-expiry of old subtitles
/// - Maximum visible line count
/// </summary>
public class SubtitleQueue
{
    private readonly ConcurrentQueue<SubtitleEntry> _incoming = new();
    private readonly List<SubtitleEntry> _visible = new();

    /// <summary>Maximum number of subtitle lines visible at once.</summary>
    public int MaxVisibleLines { get; set; } = 3;

    /// <summary>How long a final subtitle stays on screen (seconds).</summary>
    public double FinalDisplayDurationSec { get; set; } = 5.0;

    /// <summary>Enqueue a new subtitle entry (thread-safe).</summary>
    public void Enqueue(SubtitleEntry entry)
    {
        _incoming.Enqueue(entry);
    }

    /// <summary>
    /// Process incoming entries and update the visible list.
    /// Call this from the render loop.
    /// </summary>
    public IReadOnlyList<SubtitleEntry> Update()
    {
        // Drain incoming queue
        while (_incoming.TryDequeue(out var entry))
        {
            if (entry.IsProvisional)
            {
                // Replace previous provisional entry (if any)
                _visible.RemoveAll(e => e.IsProvisional);
                _visible.Add(entry);
            }
            else
            {
                // Final: remove all provisionals, add final
                _visible.RemoveAll(e => e.IsProvisional);
                _visible.Add(entry);
            }
        }

        // Remove expired entries
        var now = DateTime.UtcNow;
        _visible.RemoveAll(e =>
            !e.IsProvisional &&
            (now - e.Timestamp).TotalSeconds > FinalDisplayDurationSec);

        // Trim to max visible
        while (_visible.Count > MaxVisibleLines)
        {
            _visible.RemoveAt(0);
        }

        return _visible.AsReadOnly();
    }
}
