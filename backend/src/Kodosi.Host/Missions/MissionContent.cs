using System.Security.Cryptography;
using System.Text.Json;
using Kodosi.Data;
using Kodosi.Security;
using Kodosi.TerminalConnections;
using Microsoft.EntityFrameworkCore;

namespace Kodosi.Missions;

public sealed partial class MissionService
{
    private const int MaximumKeyStateBytes = 3 * 1024 * 1024;
    private const int MaximumItemBytes = 64 * 1024;

    public async Task RegisterRoomKeyAsync(Guid userId, Device device, RecipientKeyWrite request, CancellationToken ct)
    {
        var key = Limits.Base64(request.PublicKey, "Room public key", 1184);
        var signature = Limits.Base64(request.Signature, "Room public key signature", 3309);
        if (key.Length != 1184 || !signatures.Verify(device.SigningPublicKey,
            RoomKeyProof(userId, device.Id, key), signature)) throw ApiException.Invalid("Invalid room public key.");
        var existing = await db.RoomRecipientKeys.SingleOrDefaultAsync(x => x.DeviceId == device.Id, ct);
        if (existing is not null)
        {
            if (!existing.PublicKey.AsSpan().SequenceEqual(key)) throw ApiException.Conflict("This device already has a different room key.");
            return;
        }
        db.RoomRecipientKeys.Add(new RoomRecipientKey { DeviceId = device.Id, UserId = userId, PublicKey = key, Signature = signature });
        await db.SaveChangesAsync(ct);
        foreach (var room in await db.Missions.Where(r => r.OwnerUserId == userId || db.MissionMembers.Any(m => m.MissionId == r.Id && m.UserId == userId)).ToListAsync(ct))
            await NotifyAsync(room, ct);
    }

    public async Task<object> RecipientKeysAsync(Guid caller, Guid userId, CancellationToken ct)
    {
        await devices.RequireRelationAsync(caller, userId, ct);
        var keys = await (from key in db.RoomRecipientKeys.AsNoTracking()
                          join device in db.Devices.AsNoTracking() on key.DeviceId equals device.Id
                          where key.UserId == userId && !device.Revoked
                          select key).ToListAsync(ct);
        return keys.Select(key => new { key.UserId, key.DeviceId, publicKey = Convert.ToBase64String(key.PublicKey), signature = Convert.ToBase64String(key.Signature) });
    }

    private static byte[] RoomKeyProof(Guid user, string device, byte[] key)
    {
        using var stream = new MemoryStream();
        stream.Write("kodosi-room-recipient-v1"u8);
        CanonicalLengthPrefixedUtf8.Write(stream, user.ToString("D"));
        CanonicalLengthPrefixedUtf8.Write(stream, device);
        Span<byte> length = stackalloc byte[4];
        System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(length, (uint)key.Length);
        stream.Write(length); stream.Write(key);
        return stream.ToArray();
    }

    public async Task<object> KeyHistoryAsync(Guid id, Guid userId, long afterVersion, CancellationToken ct)
    {
        var room = await db.Missions.AsNoTracking().SingleOrDefaultAsync(x => x.Id == id, ct) ?? throw ApiException.Missing();
        if (room.OwnerUserId != userId && !await db.MissionMembers.AnyAsync(x => x.MissionId == id && x.UserId == userId, ct)
            && !await db.MissionInvitations.AnyAsync(x => x.MissionId == id && x.UserId == userId, ct)) throw ApiException.Missing();
        if (afterVersion < 0) throw ApiException.Invalid("Invalid room key position.");
        var states = await db.MissionKeyStates.AsNoTracking().Where(x => x.MissionId == id && x.Version > afterVersion)
            .OrderBy(x => x.Version).Take(1).ToListAsync(ct);
        var members = (await MemberIdsAsync(id, ct)).Concat(await db.MissionInvitations.Where(x => x.MissionId == id).Select(x => x.UserId).ToListAsync(ct)).Distinct().ToArray();
        return new { roomId = id, ownerUserId = room.OwnerUserId, version = room.KeyVersion, members,
            states = states.Select(StateDto) };
    }

    private static object StateDto(MissionKeyState state) => new
    { state.Version, body = Convert.ToBase64String(state.Body), signature = Convert.ToBase64String(state.Signature) };

    public async Task StoreKeysAsync(Guid id, Guid userId, RoomKeyWrite request, CancellationToken ct)
    {
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var room = await RequireMemberForUpdateAsync(id, userId, ct);
        await AppendKeysAsync(room, userId, request, ct);
        await transaction.CommitAsync(ct);
        await NotifyAsync(room, ct);
        foreach (var user in await MemberIdsAsync(id, ct)) connections.Notify(user, "rooms");
    }

    private async Task AppendKeysAsync(Mission room, Guid userId, RoomKeyWrite? request, CancellationToken ct)
    {
        if (request is null)
        {
            if (room.KeyVersion != 0) throw ApiException.Conflict("Refresh the room before changing its members.");
            return;
        }
        var device = await devices.RequireDeviceAsync(userId, request.DeviceId, ct);
        var body = Limits.Base64(request.Body, "Room keys", MaximumKeyStateBytes);
        var signature = Limits.Base64(request.Signature, "Room signature", 3309);
        if (!signatures.Verify(device.SigningPublicKey, Proofs.Tagged("kodosi-room-state-v1"u8, body), signature))
            throw ApiException.Forbidden("Invalid room signature.");
        var state = ReadBody<KeyStateBody>(body);
        var version = state.Version;
        var epoch = state.Epoch;
        if (state.RoomId != room.Id || state.OwnerUserId != room.OwnerUserId
            || state.AuthorId != userId || state.DeviceId != device.Id
            || version != room.KeyVersion + 1 || epoch < 1) throw ApiException.Conflict("Room keys changed; refresh before retrying.");
        var members = state.Members.Keys.ToHashSet();
        var expected = (await MemberIdsAsync(room.Id, ct)).Concat(await db.MissionInvitations.Where(x => x.MissionId == room.Id).Select(x => x.UserId).ToListAsync(ct)).ToHashSet();
        if (!members.SetEquals(expected)) throw ApiException.Conflict("Room membership changed; refresh before retrying.");
        var previous = room.KeyVersion == 0 ? null : await db.MissionKeyStates.AsNoTracking().SingleAsync(x => x.MissionId == room.Id && x.Version == room.KeyVersion, ct);
        if (previous is null)
        {
            if (userId != room.OwnerUserId || epoch != 1 || state.PreviousHash != "")
                throw ApiException.Invalid("The room owner must create its first keys.");
        }
        else
        {
            if (state.PreviousHash != Convert.ToHexStringLower(SHA256.HashData(previous.Body))
                || epoch < previous.Epoch || epoch > previous.Epoch + 1) throw ApiException.Conflict("Room key history changed.");
            var old = ReadBody<KeyStateBody>(previous.Body);
            var oldMembers = old.Members.Keys.ToHashSet();
            var added = members.Except(oldMembers).ToArray();
            var removed = oldMembers.Except(members).ToArray();
            if (userId != room.OwnerUserId && (added.Length != 0 || removed.Any(x => x != userId))) throw ApiException.Forbidden();
            if (removed.Length != 0 && epoch != previous.Epoch + 1) throw ApiException.Invalid("Leaving members requires new room keys.");
        }
        var wraps = state.Recipients;
        if (wraps.Length == 0 || wraps.Length > 1024 || wraps.Any(w => !members.Contains(w.UserId)))
            throw ApiException.Invalid("Invalid room recipients.");
        db.MissionKeyStates.Add(new MissionKeyState { MissionId = room.Id, Version = version, Epoch = epoch, Body = body,
            Signature = signature, UserId = userId, DeviceId = device.Id });
        room.KeyVersion = version;
        await db.SaveChangesAsync(ct);
    }

    public async Task<object> ContentAsync(Guid id, Guid userId, string? kind, long after, long? before, int? limit, CancellationToken ct)
    {
        var room = await RequireMemberAsync(id, userId, ct);
        if (after < 0 || before is <= 0 || limit is < 1 or > 200 || kind is not (null or "message" or "task" or "repository"))
            throw ApiException.Invalid("Invalid room history request.");
        var query = db.MissionItems.AsNoTracking().Where(x => x.MissionId == id && (kind == null || x.Kind == kind)
            && x.Sequence > after && (before == null || x.Sequence < before));
        var take = limit ?? 100;
        var items = before != null ? await query.OrderByDescending(x => x.Sequence).Take(take + 1).ToListAsync(ct)
            : await query.OrderBy(x => x.Sequence).Take(take + 1).ToListAsync(ct);
        var selected = new List<MissionItem>();
        var bytes = 0;
        foreach (var item in items.Take(take))
        {
            if (bytes + item.Body.Length + item.Signature.Length > 2 * 1024 * 1024) break;
            bytes += item.Body.Length + item.Signature.Length; selected.Add(item);
        }
        var more = items.Count > selected.Count;
        return new { roomId = id, sequence = room.ContentSequence, keyVersion = room.KeyVersion, hasMore = more,
            items = selected.OrderBy(x => x.Sequence).Select(ItemDto) };
    }

    private static object ItemDto(MissionItem item) => new
    { item.Id, item.Kind, item.Version, item.Sequence, item.KeyVersion, item.UserId, item.DeviceId, item.CreatedAt, item.UpdatedAt,
        body = Convert.ToBase64String(item.Body), signature = Convert.ToBase64String(item.Signature) };

    public async Task<object> PutContentAsync(Guid id, Guid itemId, Guid userId, ContentWrite request, CancellationToken ct)
    {
        var device = await devices.RequireDeviceAsync(userId, request.DeviceId, ct);
        var body = Limits.Base64(request.Body, "Room content", MaximumItemBytes);
        var signature = Limits.Base64(request.Signature, "Content signature", 3309);
        if (!signatures.Verify(device.SigningPublicKey, Proofs.Tagged("kodosi-room-content-v1"u8, body), signature))
            throw ApiException.Forbidden("Invalid room content signature.");
        var content = ReadBody<ContentBody>(body);
        var kind = content.Kind;
        var keyVersion = content.KeyVersion;
        var version = content.Version;
        if (kind is not ("message" or "task" or "repository") || content.RoomId != id
            || content.Id != itemId || content.AuthorId != userId
            || content.DeviceId != device.Id || request.ExpectedVersion < 0 || version != request.ExpectedVersion + 1)
            throw ApiException.Invalid("Invalid room content identity.");
        if (Limits.Base64(content.Nonce, "Content nonce", 12).Length != 12
            || Limits.Base64(content.Ciphertext, "Encrypted content", MaximumItemBytes).Length < 16)
            throw ApiException.Invalid("Invalid encrypted room content.");
        await using var transaction = await db.Database.BeginTransactionAsync(ct);
        var room = await RequireMemberForUpdateAsync(id, userId, ct);
        var existing = await db.MissionItems.SingleOrDefaultAsync(x => x.MissionId == id && x.Id == itemId, ct);
        if (existing is not null && existing.Body.AsSpan().SequenceEqual(body) && existing.Signature.AsSpan().SequenceEqual(signature)) return ItemDto(existing);
        if (room.KeyVersion == 0 || keyVersion != room.KeyVersion) throw ApiException.Conflict("Room keys changed; refresh before posting.");
        var epoch = await db.MissionKeyStates.Where(x => x.MissionId == id && x.Version == keyVersion).Select(x => x.Epoch).SingleAsync(ct);
        if (content.Epoch != epoch) throw ApiException.Invalid("The content belongs to another room key epoch.");
        if ((existing?.Version ?? 0) != request.ExpectedVersion || (existing is not null && (existing.Kind != kind || kind == "message")))
            throw ApiException.Conflict("This room item changed; refresh before retrying.");
        if (existing is null && kind != "message" && await db.MissionItems.CountAsync(x => x.MissionId == id && x.Kind == kind, ct) >= 4096)
            throw ApiException.Conflict("The room has reached its item limit.");
        var item = existing ?? new MissionItem { MissionId = id, Id = itemId, CreatedAt = clock.GetUtcNow() };
        item.Kind = kind; item.Version = version; item.KeyVersion = keyVersion;
        room.ContentSequence = checked(room.ContentSequence + 1);
        item.Sequence = room.ContentSequence; item.Body = body; item.Signature = signature;
        item.UserId = userId; item.DeviceId = device.Id; item.UpdatedAt = clock.GetUtcNow();
        if (existing is null) db.MissionItems.Add(item);
        await db.SaveChangesAsync(ct); await transaction.CommitAsync(ct);
        foreach (var member in await MemberIdsAsync(id, ct)) connections.Notify(member, "rooms");
        return ItemDto(item);
    }

    private static readonly JsonSerializerOptions BodyJson = new(Wire.Json)
    {
        RespectRequiredConstructorParameters = true,
        RespectNullableAnnotations = true,
        Converters = { new CanonicalGuidConverter() }
    };
    private static T ReadBody<T>(byte[] body) where T : class => JsonSerializer.Deserialize<T>(body, BodyJson) ?? throw ApiException.Invalid("Invalid room data.");
    private sealed record SealedBody(string Nonce, string Ciphertext);
    private sealed record RecipientBody(Guid UserId, string DeviceId, string KemCiphertext, SealedBody Sealed);
    private sealed record KeyStateBody(Guid RoomId, Guid OwnerUserId, Guid AuthorId, string DeviceId,
        long Version, long Epoch, long CreatedAtMs, string PreviousHash, Dictionary<Guid, JsonElement> Members,
        RecipientBody[] Recipients, SealedBody? PreviousKey);
    private sealed record ContentBody(Guid RoomId, Guid Id, string Kind, long Version, long KeyVersion,
        long Epoch, Guid AuthorId, string DeviceId, string Nonce, string Ciphertext);

    public sealed record RecipientKeyWrite(string PublicKey, string Signature);
    public sealed record RoomKeyWrite(string DeviceId, string Body, string Signature);
    public sealed record ContentWrite(long ExpectedVersion, string DeviceId, string Body, string Signature);
    public sealed record MembershipWrite(RoomKeyWrite? Keys);
}
