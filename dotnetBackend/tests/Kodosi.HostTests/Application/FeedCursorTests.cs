using System.Text;
using Kodosi.Application;
using Kodosi.Domain;

namespace Kodosi.HostTests;

public sealed class FeedCursorTests
{
    [Fact]
    public void V2_Cursor_RoundTrips_SubMillisecond_Precision()
    {
        var startedAt = new DateTimeOffset(
            2026,
            7,
            18,
            1,
            2,
            3,
            TimeSpan.Zero).AddTicks(4560);
        var cursor = new FeedCursor(startedAt, Guid.NewGuid());

        var decoded = FeedCursor.Decode(cursor.Encode());

        Assert.Equal(cursor, decoded);
    }

    [Fact]
    public void V2_Cursor_Does_Not_Skip_An_Older_Item_In_The_Same_Millisecond()
    {
        var millisecond = DateTimeOffset.FromUnixTimeMilliseconds(1_752_806_523_123);
        var boundary = millisecond.AddTicks(9000);
        var nextItem = millisecond.AddTicks(4000);

        var decoded = FeedCursor.Decode(
            new FeedCursor(boundary, Guid.NewGuid()).Encode());

        Assert.NotNull(decoded);
        Assert.True(nextItem < decoded.StartedAt);
    }

    [Fact]
    public void Decode_Rejects_Removed_Millisecond_Cursor_As_Invalid()
    {
        var startedAt = DateTimeOffset.FromUnixTimeMilliseconds(1_752_806_523_123);
        var id = Guid.NewGuid();
        var legacy = Convert.ToBase64String(
            Encoding.UTF8.GetBytes($"{startedAt.ToUnixTimeMilliseconds()}_{id:D}"));

        var error = Assert.Throws<DomainException>(() => FeedCursor.Decode(legacy));

        Assert.Equal("The feed cursor is invalid.", error.Message);
    }

    [Theory]
    [InlineData("not-base64")]
    [InlineData("aW52YWxpZA==")]
    public void Decode_Rejects_Malformed_Cursor(string cursor)
    {
        var error = Assert.Throws<DomainException>(() => FeedCursor.Decode(cursor));

        Assert.Equal("The feed cursor is invalid.", error.Message);
    }

    [Fact]
    public void Decode_Rejects_Overflowing_Timestamp()
    {
        var raw = $"v2_{long.MaxValue}_{Guid.NewGuid():D}";
        var cursor = Convert.ToBase64String(Encoding.UTF8.GetBytes(raw));

        var error = Assert.Throws<DomainException>(() => FeedCursor.Decode(cursor));

        Assert.Equal("The feed cursor is invalid.", error.Message);
    }
}
