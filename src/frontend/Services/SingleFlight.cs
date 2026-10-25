namespace Cleanup.Services;

/// <summary>Reuses outstanding work, with a hard cap even when synchronous I/O never returns.</summary>
internal sealed class SingleFlight<TKey, TResult>(int maximum) where TKey : notnull
{
    private readonly object _gate = new();
    private readonly Dictionary<TKey, Task<TResult>> _running = [];

    public Task<TResult> Run(TKey key, Func<TResult> work)
    {
        lock (_gate)
        {
            if (_running.TryGetValue(key, out Task<TResult>? existing) && !existing.IsCompleted) return existing;
            foreach (TKey completed in _running.Where(pair => pair.Value.IsCompleted).Select(pair => pair.Key).ToArray())
                _running.Remove(completed);
            if (_running.Count >= maximum) return Task.FromException<TResult>(new IOException("Storage is still responding to earlier requests. Try again later."));
            Task<TResult> task = Task.Factory.StartNew(work, CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
            _running[key] = task;
            return task;
        }
    }
}
