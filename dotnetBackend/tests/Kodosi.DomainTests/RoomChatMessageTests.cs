using Kodosi.Domain;

namespace Kodosi.DomainTests;

public sealed class RoomChatMessageTests
{
    [Fact]
    public void Create_Rejects_Oversized_Recipients_Without_Enumeration()
    {
        var recipients = new NonEnumeratingGuidCollection(
            RoomInputRules.ChatRecipientMaxCount + 1);

        Assert.Throws<DomainException>(() => RoomChatMessage.Create(
            Guid.NewGuid(),
            RoomId.From(Guid.NewGuid()),
            UserId.New(),
            null,
            RoomChatAuthorKind.Human,
            recipients,
            null,
            "ciphertext",
            1));
    }

    private sealed class NonEnumeratingGuidCollection(int count)
        : IReadOnlyCollection<Guid>
    {
        public int Count { get; } = count;
        public IEnumerator<Guid> GetEnumerator() =>
            throw new InvalidOperationException("Must not enumerate.");
        System.Collections.IEnumerator System.Collections.IEnumerable.GetEnumerator() =>
            GetEnumerator();
    }
}
