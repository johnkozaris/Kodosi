using System.Diagnostics.CodeAnalysis;

namespace Kodosi.Host.Realtime;

internal sealed class BoundedFifoMap<TKey, TValue>
    where TKey : notnull
{
    private readonly int _capacity;
    private readonly Dictionary<TKey, TValue> _entries = [];
    private readonly Queue<TKey> _order = new();

    public BoundedFifoMap(int capacity)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(capacity);
        _capacity = capacity;
    }

    public bool TryGetValue(TKey key, [MaybeNullWhen(false)] out TValue value)
    {
        return _entries.TryGetValue(key, out value);
    }

    public void Set(TKey key, TValue value)
    {
        if (!_entries.ContainsKey(key))
        {
            while (_entries.Count >= _capacity && _order.TryDequeue(out var oldest))
            {
                _entries.Remove(oldest);
            }
            _order.Enqueue(key);
        }
        _entries[key] = value;
    }
}
